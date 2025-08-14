/*
 * Copyright (C) 2024 Aspect
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

/*
 * LEGAL NOTICE: STRICT ADHERENCE TO THE GNU AFFERO GENERAL PUBLIC LICENSE TERMS REQUIRED
 *
 * BE IT KNOWN, that any unauthorized use, reproduction, distribution, or modification
 * of this software, in whole or in part, is a direct violation of the GNU Affero General Public
 * License (AGPL). Violators of this license will face the full force of applicable
 * international, federal, and state laws, including but not limited to copyright law,
 * intellectual property law, and contract law. Such violations will be prosecuted to
 * the maximum extent permitted by law.
 *
 * ANY INDIVIDUAL OR ENTITY FOUND TO BE IN BREACH OF THE TERMS AND CONDITIONS SET FORTH
 * IN THE GNU AFFERO GENERAL PUBLIC LICENSE WILL BE SUBJECT TO SEVERE LEGAL REPERCUSSIONS. These
 * repercussions include, but are not limited to:
 *
 * - Civil litigation seeking substantial monetary damages for all infringements,
 *   including statutory damages, actual damages, and consequential damages.
 *
 * - Injunctive relief to immediately halt any unauthorized use, distribution, or
 *   modification of this software, which may include temporary restraining orders
 *   and preliminary and permanent injunctions.
 *
 * - The imposition of criminal penalties under applicable law, including substantial
 *   fines and imprisonment.
 *
 * - Recovery of all legal fees, court costs, and associated expenses incurred in the
 *   enforcement of this license.
 *
 * YOU ARE HEREBY ADVISED to thoroughly review and comprehend the terms and conditions
 * of the GNU Affero General Public License. Ignorance of the license terms will not be accepted
 * as a defense in any legal proceedings. If you have any uncertainty or require clarification
 * regarding the license, it is strongly recommended that you consult with a qualified
 * legal professional before engaging in any activity that may be governed by the AGPL.
 *
 * FAILURE TO COMPLY with these terms will result in swift and uncompromising legal action.
 * This software is protected by copyright and other intellectual property laws. All rights,
 * including the right to seek legal remedies for any breach of this license, are expressly
 * reserved by Aspect.
 */

use log::*;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::fs::File;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::net::TcpListener;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::{spawn, AbortHandle};

use anyhow::{Context, Result};

use str0m::bwe::Bitrate;
use str0m::bwe::BweKind;
use str0m::channel::ChannelData;
use str0m::channel::ChannelId;
use str0m::format::Codec;
use str0m::media::{MediaKind, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Event, IceConnectionState, Input, Output, Rtc};

use crate::dialogs::*;
use crate::keys::Permissions;
use crate::AppState;
use crate::CreateOffer;
use crate::{ClientCommand, InputCommand};

mod pipeline;
mod tcp;

enum DatachannelMessageKind {
    Binary,
    Text
}

impl DatachannelMessageKind {
    fn is_binary(&self) -> bool {
        match self {
            DatachannelMessageKind::Binary => true,
            DatachannelMessageKind::Text => false,
        }
    }
}

struct FileTransfers {
    // The datachannel and the corresponding data
    rx: Receiver<(ChannelId, Vec<u8>, DatachannelMessageKind)>,
    tx: Sender<(ChannelId, Vec<u8>, DatachannelMessageKind)>,

    // When file chunks arrive, we send them to this sender and the task dedicated to that file
    // will handle those chunks
    inbound_transfers: Arc<Mutex<HashMap<u32, Sender<Vec<u8>>>>>,
    outbound_transfers: Arc<Mutex<HashMap<u32, AbortHandle>>>,
}

impl FileTransfers {
    fn new() -> Self {
        let (tx, rx) = channel(100);
        Self {
            tx,
            rx,
            inbound_transfers: Arc::new(Mutex::new(HashMap::new())),
            outbound_transfers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn handle_inbound_file_chunk(&self, id: u32, chunk: Vec<u8>) -> Result<()> {
        let sender = {
            let mut inbound_transfers = self.inbound_transfers.lock().unwrap();
            inbound_transfers.get_mut(&id).context("inbound file chunk received for non-existent file transfer")?.clone()
        };
        sender.send(chunk).await?;
        Ok(())
    }

    fn cancel_transfer(&self, id: u32) {
        // This suffices to cancel an inbound transfer as dropping the Sender will cause the
        // recv loop to stop, thereby ending the corresponding tokio task.
        info!("Explicitly canceling transfer {}.", id);
        self.inbound_transfers.lock().unwrap().remove(&id);
        let mut outbound_transfers = self.outbound_transfers.lock().unwrap();
        if let Some(abort_handle) = outbound_transfers.get(&id) {
            abort_handle.abort();
            info!("Aborted handle for transfer {}.", id);
        }
        outbound_transfers.remove(&id);
    }

    fn begin_inbound_transfer(
        &self,
        tx: Sender<Dialog>,
        id: u32,
        channel_id: ChannelId,
        size: u64,
    ) {
        let inbound_transfers = Arc::downgrade(&self.inbound_transfers);
        let datachannel_tx = self.tx.clone();
        spawn(async move {
            let path = spawn_file_dialog(&tx, FileDialogKind::Save).await;
            match path {
                Some(path) => {
                    let file = File::create(path).await;
                    if let Ok(mut file) = file {
                        let (chunk_tx, mut chunk_rx) = channel(100);
                        match inbound_transfers.upgrade() {
                            Some(inbound_transfers) => { inbound_transfers.lock().unwrap().insert(id, chunk_tx); },
                            None => return
                        }
                        let mut total_size = 0u64;
                        datachannel_tx
                            .send((
                                channel_id,
                                serde_json::to_vec(
                                    &serde_json::json!({ "type": "transferready", "id": id }),
                                )
                                .unwrap(),
                                DatachannelMessageKind::Text
                            ))
                            .await
                            .ok();
                        info!("Entering file write loop for transfer: {}", id);
                        while let Some(chunk) = chunk_rx.recv().await {
                            if let Err(e) = file.write_all(chunk.as_slice()).await {
                                info!("Write error: {}", e);
                                match inbound_transfers.upgrade() {
                                    Some(inbound_transfers) => { inbound_transfers.lock().unwrap().remove(&id); },
                                    None => return
                                }
                                datachannel_tx.send((
                                    channel_id,
                                    serde_json::to_vec(
                                        &serde_json::json!({ "type": "canceltransfer", "id": id }),
                                    )
                                    .unwrap(),
                                    DatachannelMessageKind::Text
                                )).await.ok();
                                spawn_message_dialog(
                                    &tx,
                                    "Tenebra File Transfer Error",
                                    format!("Failed to write file: {}", e),
                                    rfd::MessageLevel::Error,
                                )
                                .await;
                                break;
                            }
                            total_size += chunk.len() as u64;
                            if total_size >= size {
                                info!("Transfer complete, removing self.");
                                match inbound_transfers.upgrade() {
                                    Some(inbound_transfers) => { inbound_transfers.lock().unwrap().remove(&id); },
                                    None => return
                                }
                                info!("Removed!");
                                spawn_message_dialog(
                                    &tx,
                                    "Tenebra File Transfer Notification",
                                    format!(
                                        "Finished file transfer. Wrote {}/{} bytes.",
                                        total_size, size
                                    ),
                                    rfd::MessageLevel::Info,
                                )
                                .await;
                                break;
                            }
                        }
                        info!("Flushing file and exiting task.");
                        if let Err(e) = file.sync_all().await {
                            error!("Failed to sync file to disk: {}", e);
                        }
                    } else if let Err(e) = file {
                        spawn_message_dialog(
                            &tx,
                            "Tenebra File Transfer Error",
                            format!("Failed to create file: {}", e),
                            rfd::MessageLevel::Error,
                        )
                        .await;
                        datachannel_tx
                            .send((
                                channel_id,
                                serde_json::to_vec(
                                    &serde_json::json!({ "type": "canceltransfer", "id": id }),
                                )
                                .unwrap(),
                                DatachannelMessageKind::Text
                            ))
                            .await
                            .ok();
                    }
                }
                None => {
                    // Cancel the transfer
                    datachannel_tx
                        .send((
                            channel_id,
                            serde_json::to_vec(
                                &serde_json::json!({ "type": "canceltransfer", "id": id }),
                            )
                            .unwrap(),
                            DatachannelMessageKind::Text
                        ))
                        .await
                        .ok();
                }
            }
            info!("Reached end of task body!");
        });
    }

    fn begin_outbound_transfer(&self, tx: Sender<Dialog>, id: u32, channel_id: ChannelId) {
        let datachannel_tx = self.tx.clone();
        let outbound_transfers = Arc::clone(&self.outbound_transfers);
        let handle = spawn(async move {
            let path = spawn_file_dialog(&tx, FileDialogKind::Open).await;
            match path {
                Some(path) => {
                    let file = File::open(path).await;
                    if let Ok(mut file) = file {
                        let metadata = file.metadata().await;
                        if let Ok(metadata) = metadata {
                            let total_size = metadata.len();
                            datachannel_tx.send((
                                channel_id,
                                serde_json::to_vec(
                                    &serde_json::json!({ "type": "transferready", "id": id, "size": total_size }),
                                )
                                .unwrap(),
                                DatachannelMessageKind::Text
                            )).await.ok();
                            const CHUNK_SIZE: usize = 1024;
                            let mut buf = vec![0u8; CHUNK_SIZE];
                            loop {
                                let n = file.read(&mut buf).await;
                                if let Ok(n) = n {
                                    if n == 0 {
                                        spawn_message_dialog(
                                            &tx,
                                            "Tenebra File Transfer Notification",
                                            format!(
                                                "Finished file transfer. Sent {} bytes.",
                                                total_size
                                            ),
                                            rfd::MessageLevel::Info,
                                        )
                                        .await;
                                        break;
                                    }

                                    let chunk = &buf[..n];
                                    let mut v = Vec::with_capacity(4 + chunk.len());
                                    v.extend_from_slice(&id.to_be_bytes());
                                    v.extend_from_slice(chunk);

                                    if datachannel_tx.send((channel_id, v, DatachannelMessageKind::Binary)).await.is_err() {
                                        break; // Receiver closed
                                    }
                                } else {
                                    datachannel_tx.send((
                                        channel_id,
                                        serde_json::to_vec(
                                            &serde_json::json!({ "type": "canceltransfer", "id": id }),
                                        )
                                        .unwrap(),
                                        DatachannelMessageKind::Text
                                    )).await.ok();
                                }
                            }
                        } else if let Err(e) = metadata {
                            spawn_message_dialog(
                                &tx,
                                "Tenebra File Transfer Error",
                                format!("Failed to query metadata of file file: {}", e),
                                rfd::MessageLevel::Error,
                            )
                            .await;
                            datachannel_tx
                                .send((
                                    channel_id,
                                    serde_json::to_vec(
                                        &serde_json::json!({ "type": "canceltransfer", "id": id }),
                                    )
                                    .unwrap(),
                                    DatachannelMessageKind::Text
                                ))
                                .await
                                .ok();
                        }
                    } else if let Err(e) = file {
                        spawn_message_dialog(
                            &tx,
                            "Tenebra File Transfer Error",
                            format!("Failed to open file: {}", e),
                            rfd::MessageLevel::Error,
                        )
                        .await;
                        datachannel_tx
                            .send((
                                channel_id,
                                serde_json::to_vec(
                                    &serde_json::json!({ "type": "canceltransfer", "id": id }),
                                )
                                .unwrap(),
                                DatachannelMessageKind::Text
                            ))
                            .await
                            .ok();
                    }
                }
                None => {
                    // Cancel the transfer
                    datachannel_tx
                        .send((
                            channel_id,
                            serde_json::to_vec(
                                &serde_json::json!({ "type": "canceltransfer", "id": id }),
                            )
                            .unwrap(),
                            DatachannelMessageKind::Text
                        ))
                        .await
                        .ok();
                }
            }
            outbound_transfers.lock().unwrap().remove(&id);
        });
        // This is technically a race condition: it's possible for the `remove` call above to run
        // before this call, but the cost of fixing it is not worth it, and it's functionally
        // impossible to trigger.
        self.outbound_transfers
            .lock()
            .unwrap()
            .insert(id, handle.abort_handle());
    }

    async fn recv(&mut self) -> (ChannelId, Vec<u8>, DatachannelMessageKind) {
        // .unwrap() is safe here because as long as `self` exists, so do `tx` and `rx`
        self.rx.recv().await.unwrap()
    }
}

pub async fn run(
    mut rtc: Rtc,
    udp_socket: UdpSocket,
    tcp_listener: TcpListener,
    local_socket_addr: SocketAddr,
    tcp_addr: SocketAddr,
    state: AppState,
    offer: CreateOffer,
    permissions: Permissions,
) -> Result<()> {
    let mut buf = Vec::new();

    let mut listener = tcp::Listener::listen(tcp_listener)?;

    let mut file_transfers = FileTransfers::new();

    let mut video: (pipeline::ScreenRecordingPipeline, Option<Mid>) = (
        pipeline::ScreenRecordingPipeline::new(state.config.clone(), offer.show_mouse)?,
        None,
    );
    let mut audio: (pipeline::AudioRecordingPipeline, Option<Mid>) =
        (pipeline::AudioRecordingPipeline::new().await?, None);

    let mut can_write_channel = true;

    let ret = loop {
        // Poll output until we get a timeout. The timeout means we are either awaiting UDP socket input
        // or the timeout to happen.
        let output = rtc.poll_output()?;
        let time = match output {
            Output::Timeout(v) => v,

            Output::Transmit(v) => {
                match v.proto {
                    Protocol::Tcp => listener.send(&v.contents, v.destination).await?,
                    Protocol::Udp => {
                        if let Err(e) = udp_socket.send_to(&v.contents, v.destination).await {
                            warn!("Error sending UDP data: {}", e);
                        }
                    }
                    p => warn!("Unimplemented protocol: {}", p),
                }

                continue;
            }

            Output::Event(v) => {
                match v {
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        break Ok(());
                    }
                    Event::MediaAdded(media_added) => {
                        let kind = media_added.kind;
                        cfg_if::cfg_if! {
                            if #[cfg(any(target_os = "linux", target_os = "windows"))] {
                                if kind.is_audio() && !state.config.sound_forwarding {
                                    continue
                                }
                            } else {
                                if kind.is_audio() {
                                    continue
                                }
                            }
                        }

                        rtc.bwe()
                            .set_desired_bitrate(Bitrate::kbps(state.config.target_bitrate as u64));

                        match kind {
                            MediaKind::Video => {
                                video.0.start_pipeline();
                                video.1 = Some(media_added.mid);
                            }
                            MediaKind::Audio => {
                                audio.0.start_pipeline();
                                audio.1 = Some(media_added.mid);
                            }
                        }
                    }
                    Event::KeyframeRequest(_) => {
                        video.0.force_keyframe();
                    }
                    Event::EgressBitrateEstimate(
                        BweKind::Twcc(bitrate) | BweKind::Remb(_, bitrate),
                    ) => {
                        let mut bwe = (bitrate.as_u64() / 1000)
                            .clamp(500, state.config.target_bitrate as u64 + 3000)
                            as u32;
                        if audio.1.is_some() {
                            bwe -= 64;
                        }

                        video.0.set_bitrate(bwe);

                        rtc.bwe().set_current_bitrate(Bitrate::kbps(bwe as _));
                        debug!("Set current bitrate to {}", bwe);
                    }
                    Event::ChannelData(ChannelData {
                        data,
                        binary,
                        id: channel_id,
                        ..
                    }) => {
                        if !binary {
                            let msg_str = String::from_utf8(data)?;
                            let cmd: ClientCommand = serde_json::from_str(&msg_str)?;
                            trace!("Client command: {:#?}", cmd);

                            match permissions {
                                Permissions::FullControl => match cmd.r#type.as_str() {
                                    "requesttransfer" => {
                                        match (cmd.size, cmd.id) {
                                            // inbound
                                            (Some(size), Some(id)) => {
                                                file_transfers.begin_inbound_transfer(
                                                    state.dialog_tx.clone(),
                                                    id as _,
                                                    channel_id,
                                                    size,
                                                );
                                            }
                                            // outbound
                                            (None, Some(id)) => file_transfers.begin_outbound_transfer(state.dialog_tx.clone(), id as _, channel_id),
                                            _ => warn!(
                                                "Malformed `requesttransfer` packet: {}",
                                                msg_str
                                            ),
                                        }
                                    }
                                    "transferready" => warn!("Received `transferready` packet despite being server. Perhaps update tenebra?"),
                                    "canceltransfer" => file_transfers.cancel_transfer(cmd.id.context("no id present on canceltransfer packet")? as _),
                                    _ => {
                                        state
                                            .input_tx
                                            .send(InputCommand::ClientCommand(cmd))
                                            .await?
                                    }
                                },
                                _ => error!("Rejected input command: {:?}", cmd),
                            }
                        } else {
                            // File segment packet
                            let id = u32::from_be_bytes(data[0..4].try_into()?);
                            let chunk = data[4..].to_vec();
                            file_transfers.handle_inbound_file_chunk(id, chunk).await?;
                        }
                    }
                    Event::IceConnectionStateChange(connection_state) => {
                        info!("New state: {:?}", connection_state);
                        if connection_state == IceConnectionState::Connected {
                            info!("ICE Connection state is now CONNECTED. Waiting for media to be added...");
                        }
                    }
                    Event::ChannelBufferedAmountLow(_) => can_write_channel = true,
                    Event::ChannelOpen(id, _) => {
                        if let Some(mut channel) = rtc.channel(id) {
                            channel.set_buffered_amount_low_threshold(32768)?;
                        }
                    }
                    _ => {}
                }
                continue;
            }
        };

        let timeout = time - Instant::now();

        if timeout.is_zero() {
            rtc.handle_input(Input::Timeout(Instant::now()))?;
            continue;
        }

        buf.resize(2000, 0);

        let input = tokio::select! {
            _ = tokio::time::sleep_until(time.into()) => Input::Timeout(Instant::now()),
            (channel_id, data, kind) = file_transfers.recv(), if can_write_channel => {
                let channel = rtc.channel(channel_id);
                if let Some(mut channel) = channel {
                    channel.write(kind.is_binary(), &data)?;
                    if channel.buffered_amount()? > 32768 {
                        can_write_channel = false;
                    }
                } else {
                    warn!("Got file chunk headed to non-existent channel: {:?}", channel_id);
                }
                Input::Timeout(Instant::now())
            }
            Some((buf, pts)) = video.0.recv_frame(), if video.1.is_some() => {
                let writer = rtc
                    .writer(video.1.unwrap())
                    .context("couldn't get rtc writer")?
                    .playout_delay(MediaTime::ZERO, MediaTime::ZERO);
                let pt = writer
                    .payload_params()
                    .find(|&params| params.spec().codec == Codec::H264)
                    .unwrap()
                    .pt();
                let now = Instant::now();
                writer.write(pt, now, MediaTime::from_micros(pts), buf)?;
                Input::Timeout(Instant::now())
            }
            Some((buf, pts)) = audio.0.recv_frame(), if audio.1.is_some() => {
                let writer = rtc
                    .writer(audio.1.unwrap())
                    .context("couldn't get rtc writer")?
                    .playout_delay(MediaTime::ZERO, MediaTime::ZERO);
                let pt = writer
                    .payload_params()
                    .find(|&params| params.spec().codec == Codec::Opus)
                    .unwrap()
                    .pt();
                let now = Instant::now();
                writer.write(pt, now, MediaTime::from_micros(pts), buf)?;
                Input::Timeout(Instant::now())
            }
            Some((msg, addr)) = listener.read() => {
                buf = msg;
                Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Tcp,
                        source: addr,
                        destination: tcp_addr,
                        contents: buf.as_slice().try_into()?,
                    },
                )
            }
            msg = udp_socket.recv_from(&mut buf) => {
                match msg {
                    Ok((n, source)) => {
                        // UDP data received.
                        Input::Receive(
                            Instant::now(),
                            Receive {
                                proto: Protocol::Udp,
                                source,
                                destination: SocketAddr::new(
                                    local_socket_addr.ip(),
                                    udp_socket.local_addr()?.port(),
                                ),
                                contents: (&buf[..n]).try_into()?,
                            },
                        )
                    }
                    Err(e) => match e.kind() {
                        ErrorKind::ConnectionReset => continue,
                        _ => {
                            error!("webrtc network error {:?}", e);
                            break Err(e.into());
                        }
                    }
                }
            }
        };

        rtc.handle_input(input)?;
    };

    ret
}
