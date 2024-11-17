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

use anyhow::anyhow;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use str0m::bwe::Bitrate;
use str0m::bwe::BweKind;
use str0m::channel::ChannelData;
use tokio::net::TcpListener;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::Notify;

use anyhow::Context;
use str0m::media::MediaAdded;
use str0m::media::MediaTime;
use str0m::net::Protocol;
use str0m::net::Receive;
use str0m::{Event, IceConnectionState, Input, Output, Rtc};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::AppState;
use crate::CreateOffer;
use crate::InputCommand;

mod pipeline;
mod tcp;

pub enum GStreamerControlMessage {
    Stop,
    RequestKeyFrame,
    Bitrate(u32),
    Stats { rtt: Option<f32>, loss: Option<f32> },
}

struct GStreamerInstance {
    buffer_rx: UnboundedReceiver<(Vec<u8>, u64)>,
    control_tx: UnboundedSender<GStreamerControlMessage>,
    media: MediaAdded,
}

impl Drop for GStreamerInstance {
    fn drop(&mut self) {
        self.control_tx.send(GStreamerControlMessage::Stop).ok();
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
    mut kill_rx: UnboundedReceiver<()>,
) -> anyhow::Result<()> {
    let mut buf = Vec::new();

    let mut listener = tcp::Listener::listen(tcp_listener)?;

    let mut gstreamers: Vec<GStreamerInstance> = vec![];

    let waker = Arc::new(Notify::new());

    let ret = loop {
        if kill_rx.try_recv().is_ok() {
            break Err(anyhow!("task killed from the kill_tx"));
        }

        for gstreamer in gstreamers.iter_mut() {
            let buf = gstreamer.buffer_rx.try_recv();

            if let Ok((buf, pts)) = buf {
                let writer = rtc
                    .writer(gstreamer.media.mid)
                    .context("couldn't get rtc writer")?
                    .playout_delay(MediaTime::ZERO, MediaTime::ZERO);
                let pt = writer.payload_params().nth(0).unwrap().pt();
                let now = Instant::now();
                writer.write(pt, now, MediaTime::from_micros(pts), buf)?;
            }
        }

        // Poll output until we get a timeout. The timeout means we are either awaiting UDP socket input
        // or the timeout to happen.
        let output = rtc.poll_output()?;
        let time = match output {
            Output::Timeout(v) => v,

            Output::Transmit(v) => {
                match v.proto {
                    Protocol::Tcp => listener.send(&v.contents, v.destination).await?,
                    Protocol::Udp => {
                        udp_socket.send_to(&v.contents, v.destination).await.ok();
                    }
                    p => println!("Unimplemented protocol: {}", p),
                }

                continue;
            }

            Output::Event(v) => {
                //println!("Received RTP event: {:?}", v);
                match v {
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        break Ok(());
                    }
                    Event::MediaAdded(media_added) => {
                        #[cfg(feature = "vaapi")]
                        rtc.direct_api()
                            .stream_tx_by_mid(media_added.mid, None)
                            .context("no stream")?
                            .set_unpaced(true);
                        rtc.bwe()
                            .set_desired_bitrate(Bitrate::kbps(state.bitrate as u64));
                        let (control_tx, control_rx) =
                            unbounded_channel::<GStreamerControlMessage>();
                        let (buffer_tx, buffer_rx) = unbounded_channel();
                        gstreamers.push(GStreamerInstance {
                            buffer_rx,
                            control_tx,
                            media: media_added,
                        });
                        let waker_clone = waker.clone();
                        tokio::task::spawn(pipeline::start_pipeline(
                            state.startx,
                            offer.show_mouse,
                            control_rx,
                            buffer_tx,
                            waker_clone,
                        ));
                    }
                    Event::MediaEgressStats(stats) => {
                        for gstreamer in gstreamers.iter_mut() {
                            gstreamer.control_tx.send(GStreamerControlMessage::Stats {
                                rtt: stats.rtt,
                                loss: stats.loss,
                            })?;
                        }
                    }
                    Event::KeyframeRequest(_) => {
                        for gstreamer in gstreamers.iter_mut() {
                            gstreamer
                                .control_tx
                                .send(GStreamerControlMessage::RequestKeyFrame)?;
                        }
                    }
                    Event::EgressBitrateEstimate(
                        BweKind::Twcc(bitrate) | BweKind::Remb(_, bitrate),
                    ) => {
                        #[cfg(feature = "vaapi")]
                        let bwe = (bitrate.as_u64() / 1000).clamp(4000, state.bitrate as u64 + 3000)
                            as u32;
                        #[cfg(not(feature = "vaapi"))]
                        let bwe = (bitrate.as_u64() / 1000).clamp(2000, state.bitrate as u64 + 3000)
                            as u32;
                        for gstreamer in gstreamers.iter_mut() {
                            gstreamer
                                .control_tx
                                .send(GStreamerControlMessage::Bitrate(bwe))?;
                        }
                        rtc.bwe().set_current_bitrate(Bitrate::kbps(bwe as _));
                    }
                    Event::ChannelData(ChannelData { data, .. }) => {
                        let msg_str = String::from_utf8(data)?;
                        let cmd: InputCommand = serde_json::from_str(&msg_str)?;
                        state.input_tx.send(cmd)?;
                    }
                    Event::IceConnectionStateChange(state) => {
                        println!("New state: {:?}", state);
                        if state == IceConnectionState::Connected {
                            println!("ICE Connection state is now CONNECTED. Waiting for media to be added...");
                        }
                    }
                    _ => {}
                }
                continue;
            }
        };

        let timeout = time - Instant::now();

        // socket.set_read_timeout(Some(0)) is not ok
        if timeout.is_zero() {
            rtc.handle_input(Input::Timeout(Instant::now()))?;
            continue;
        }

        //socket.set_read_timeout(Some(timeout))?;
        buf.resize(2000, 0);

        let input = tokio::select! {
            _ = tokio::time::sleep_until(time.into()) => Input::Timeout(Instant::now()),
            _ = waker.notified() => Input::Timeout(Instant::now()),
            Some((msg, addr)) = listener.read() => {
                buf = msg;
                Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Tcp,
                        source: addr,
                        destination: tcp_addr,
                        contents: (&buf).as_slice().try_into()?,
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
                            println!("webrtc network error {:?}", e);
                            break Err(e.into());
                        }
                    }
                }
            }
        };

        rtc.handle_input(input)?;
    };

    for gstreamer in gstreamers {
        gstreamer
            .control_tx
            .send(GStreamerControlMessage::Stop)
            .ok();
    }

    ret
}
