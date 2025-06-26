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
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::time::Instant;
use str0m::bwe::Bitrate;
use str0m::bwe::BweKind;
use str0m::channel::ChannelData;
use str0m::format::Codec;
use str0m::media::{MediaKind, MediaTime, Mid};
use tokio::net::TcpListener;
use tokio::net::UdpSocket;

use anyhow::Context;
use str0m::net::{Protocol, Receive};
use str0m::{Event, IceConnectionState, Input, Output, Rtc};

use crate::keys::Permissions;
use crate::AppState;
use crate::CreateOffer;
use crate::InputCommand;

mod pipeline;
mod tcp;

pub async fn run(
    mut rtc: Rtc,
    udp_socket: UdpSocket,
    tcp_listener: TcpListener,
    local_socket_addr: SocketAddr,
    tcp_addr: SocketAddr,
    state: AppState,
    offer: CreateOffer,
    permissions: Permissions,
) -> anyhow::Result<()> {
    let mut buf = Vec::new();

    let mut listener = tcp::Listener::listen(tcp_listener)?;

    // Initialize Wayland screencast if running under Wayland
    #[cfg(target_os = "linux")]
    {
        use crate::wayland::{DisplayProtocol, setup_wayland_screencast};
        let display_protocol = DisplayProtocol::detect();
        if display_protocol == DisplayProtocol::Wayland {
            info!("Initializing Wayland screencast...");
            if let Err(e) = setup_wayland_screencast().await {
                error!("Failed to initialize Wayland screencast: {}", e);
                anyhow::bail!("Wayland screencast initialization failed: {}", e);
            }
            info!("Wayland screencast initialized successfully");
        }
    }

    let mut video: (pipeline::ScreenRecordingPipeline, Option<Mid>) = (
        pipeline::ScreenRecordingPipeline::new(state.config.clone(), offer.show_mouse)?,
        None,
    );
    let mut audio: (pipeline::AudioRecordingPipeline, Option<Mid>) =
        (pipeline::AudioRecordingPipeline::new().await?, None);

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
                        udp_socket.send_to(&v.contents, v.destination).await.ok();
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
                    Event::ChannelData(ChannelData { data, .. }) => {
                        let msg_str = String::from_utf8(data)?;
                        let cmd: InputCommand = serde_json::from_str(&msg_str)?;
                        trace!("Input command: {:#?}", cmd);
                        match permissions {
                            Permissions::FullControl => {
                                state.input_tx.send(cmd)?;
                            }
                            _ => error!("Rejected input command: {:?}", cmd),
                        }
                    }
                    Event::IceConnectionStateChange(connection_state) => {
                        info!("New state: {:?}", connection_state);
                        if connection_state == IceConnectionState::Connected {
                            info!("ICE Connection state is now CONNECTED. Waiting for media to be added...");
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
            },
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
            },
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
