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
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;

use base64::prelude::*;

use bytes::BufMut;
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use webrtc::api::media_engine::MIME_TYPE_OPUS;
use webrtc::rtcp::transport_feedbacks::transport_layer_nack::TransportLayerNack;
use webrtc::rtp::header::Header;
use webrtc::rtp::header::EXTENSION_PROFILE_ONE_BYTE;
use webrtc::rtp::packet::Packet as RTPPacket;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::sdp::description::common::Attribute;
use webrtc::sdp::util::Codec;
use webrtc::sdp::MediaDescription;
use webrtc::util::Marshal;
use webrtc::util::Unmarshal;

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::task::spawn_blocking;

use webrtc::api::interceptor_registry::configure_nack;
use webrtc::api::interceptor_registry::configure_rtcp_reports;
use webrtc::api::interceptor_registry::configure_twcc;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::packet::Packet;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpHeaderExtensionCapability};
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::Error;

use crate::AppState;
use crate::CreateOffer;

mod pipeline;
mod playout_delay;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputCommand {
    pub r#type: String,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub button: Option<u8>,
    pub key: Option<String>,
}

fn configure_playout_delay(
    mut registry: Registry,
    media_engine: &mut MediaEngine,
) -> Result<Registry> {
    media_engine.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: String::from("http://www.webrtc.org/experiments/rtp-hdrext/playout-delay"),
        },
        webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video,
        None,
    )?;
    let sender = Box::new(playout_delay::Sender::builder());
    registry.add(sender);
    Ok(registry)
}

pub async fn start_video_streaming(
    offer: CreateOffer,
    offer_tx: tokio::sync::mpsc::Sender<String>,
    state: AppState,
) -> Result<(), anyhow::Error> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    // need redundant fec support for ulp (chrome is stupid??)
    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: "video/red".to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 112,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;

    // we need to decode their offer NOW in order to fill the apt field on the fmtp
    let desc_data = BASE64_STANDARD.decode(offer.offer)?;
    let desc_data = std::str::from_utf8(&desc_data)?;
    let their_offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    let their_desc = their_offer.unmarshal()?;
    let red_pt = their_desc.get_payload_type_for_codec(&Codec {
        name: "red".to_string(),
        clock_rate: 90000,
        rtcp_feedback: vec![],
        ..Default::default()
    })?;
    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: "video/rtx".to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: format!("apt={}", red_pt),
                rtcp_feedback: vec![],
            },
            payload_type: 113,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;
    let mut registry = Registry::new();

    // drop this garbage nack implementation, what's the point of implementing nack
    // if it only works with your own goofy non-compliant retransmission scheme?

    //registry = configure_nack(registry, &mut m);
    registry = configure_rtcp_reports(registry);
    // we only need twcc to get the client's jitter value
    registry = configure_twcc(registry, &mut m)?;
    registry = configure_playout_delay(registry, &mut m)?;
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let peer_connection = api.new_peer_connection(config).await?;

    let video_track = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: "video/red".to_owned(),
            clock_rate: 90000,
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let rtp_sender = peer_connection
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // last N packets for nack functionality
    const NACK_CAPACITY: usize = 100;
    let nackable_packets = Arc::new(Mutex::new(VecDeque::new()));
    let nackable_packets_clone = nackable_packets.clone();
    let video_track_clone = video_track.clone();
    let rtx_ssrc = rand::random::<u32>();
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        let mut nack_sequence_number = 0;
        while let Ok((packets, _)) = rtp_sender.read(&mut rtcp_buf).await {
            //   0                   1                   2                   3
            //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
            // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
            // |            PID                |             BLP               |
            // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

            //             Figure 4: Syntax for the Generic NACK message

            // Packet ID (PID): 16 bits
            //    The PID field is used to specify a lost packet.  The PID field
            //    refers to the RTP sequence number of the lost packet.
            // bitmask of following lost packets (BLP): 16 bits
            //    The BLP allows for reporting losses of any of the 16 RTP packets
            //    immediately following the RTP packet indicated by the PID.  The
            //    BLP's definition is identical to that given in [6].  Denoting the
            //    BLP's least significant bit as bit 1, and its most significant bit
            //    as bit 16, then bit i of the bit mask is set to 1 if the receiver
            //    has not received RTP packet number (PID+i) (modulo 2^16) and
            //    indicates this packet is lost; bit i is set to 0 otherwise.  Note
            //    that the sender MUST NOT assume that a receiver has received a
            //    packet because its bit mask was set to 0.  For example, the least
            //    significant bit of the BLP would be set to 1 if the packet
            //    corresponding to the PID and the following packet have been lost.
            //    However, the sender cannot infer that packets PID+2 through PID+16
            //    have been received simply because bits 2 through 15 of the BLP are
            //    0; all the sender knows is that the receiver has not reported them
            //    as lost at this time.

            for packet in packets {
                if let Some(e) = packet.as_any().downcast_ref::<TransportLayerNack>() {
                    for nack in &e.nacks {
                        dbg!(&nack);
                        // first, find the main packet
                        let nackable_packets = nackable_packets_clone.lock().await;
                        if let Some((idx, main_packet)) = nackable_packets.iter().enumerate().find(
                            |possible_packet: &(_, &RTPPacket)| {
                                possible_packet.1.header.sequence_number == nack.packet_id
                            },
                        ) {
                            let make_rtx_packet =
                                |packet: &RTPPacket,
                                 nack_sequence_number: &mut u16|
                                 -> anyhow::Result<RTPPacket> {
                                    // construct the rtx packet
                                    // add OSN (seen below)
                                    let mut payload = BytesMut::new();
                                    payload.put_u16(packet.header.sequence_number);
                                    payload.extend_from_slice(packet.payload.as_ref());
                                    let rtx_packet = RTPPacket {
                                        header: Header {
                                            version: 2,
                                            padding: true,
                                            extension: false,
                                            marker: true,
                                            payload_type: 113,
                                            sequence_number: *nack_sequence_number,
                                            timestamp: packet.header.timestamp,
                                            // set by webrtc-rs
                                            ssrc: rtx_ssrc,
                                            csrc: vec![],
                                            extension_profile: EXTENSION_PROFILE_ONE_BYTE,
                                            extensions: vec![],
                                            extensions_padding: 0,
                                        },
                                        payload: payload.freeze(),
                                    };
                                    *nack_sequence_number += 1;
                                    //println!("{}", rtx_packet);
                                    Ok(rtx_packet)
                                };

                            video_track_clone
                                .write_rtp(&make_rtx_packet(
                                    main_packet,
                                    &mut nack_sequence_number,
                                )?)
                                .await?;

                            // find the other packets
                            for i in 0..16 {
                                if nack.lost_packets & (1 << i) != 0 {
                                    // ASSUME that the seqnums are actually in order
                                    if let Some(packet) = nackable_packets.get(idx + i) {
                                        assert_eq!(
                                            packet.header.sequence_number,
                                            main_packet.header.sequence_number + i as u16,
                                        );
                                        video_track_clone
                                            .write_rtp(&make_rtx_packet(
                                                packet,
                                                &mut nack_sequence_number,
                                            )?)
                                            .await?;
                                    }
                                }
                            }
                        }
                        //  The format of a retransmission packet is shown below:
                        //  0                   1                   2                   3
                        //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
                        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
                        // |                         RTP Header                            |
                        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
                        // |            OSN                |                               |
                        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+                               |
                        // |                  Original RTP Packet Payload                  |
                        // |                                                               |
                        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
                    }
                }
            }
        }
        Result::<()>::Ok(())
    });
    let (done_tx, mut done_rx) = tokio::sync::broadcast::channel::<()>(1);
    {
        *state.kill_switch.lock().unwrap() = Some(done_tx.clone());
    }

    let done_tx_clone = done_tx.clone();
    peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        println!("Peer Connection State has changed: {s}");
        if s == RTCPeerConnectionState::Failed {
            // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
            // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
            // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
            println!("Peer Connection has gone to failed exiting: Done forwarding");
            let _ = done_tx_clone.send(());
        }
        Box::pin(async {})
    }));
    let done_tx_clone = done_tx.clone();
    let input_tx1 = state.input_tx.clone();
    peer_connection.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        let d_label = d.label().to_owned();
        let d_id = d.id();
        println!("New DataChannel {d_label} {d_id}");

        // Register channel opening handling
        let done_tx4 = done_tx_clone.clone();
        let done_tx5 = done_tx_clone.clone();
        let input_tx2 = input_tx1.clone();
        Box::pin(async move {
            let d_label2 = d_label.clone();
            let d_id2 = d_id;
            d.on_close(Box::new(move || {
                println!("Data channel closed");
                let _ = done_tx4.send(());
                Box::pin(async {})
            }));

            d.on_open(Box::new(move || {
                println!("Data channel '{d_label2}'-'{d_id2}' open.");

                Box::pin(async move {})
            }));

            // Register text message handling
            d.on_message(Box::new(move |msg: DataChannelMessage| {
                let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
                let cmd: InputCommand = match serde_json::from_str(&msg_str) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        dbg!(e);
                        let _ = done_tx5.send(());
                        return Box::pin(async {});
                    }
                };

                let _ = input_tx2.send(cmd);

                Box::pin(async {})
            }));
        })
    }));

    //println!("--------------------\nClient's SDP:\n{}", their_offer.sdp);
    peer_connection.set_remote_description(their_offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(answer).await?;
    let _ = gather_complete.recv().await;
    let (ulp_pt, h264_pt, ssrc) =
        if let Some(local_desc) = peer_connection.local_description().await {
            let mut parsed_desc = local_desc.unmarshal()?;
            let ulp_pt = parsed_desc.get_payload_type_for_codec(&Codec {
                name: "ulpfec".to_string(),
                clock_rate: 90000,
                rtcp_feedback: vec![],
                ..Default::default()
            })?;
            // first try to get one with the exact same fmtp
            let h264_pt = parsed_desc
                .get_payload_type_for_codec(&Codec {
                    name: "H264".to_string(),
                    fmtp: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                        .to_string(),
                    clock_rate: 90000,
                    ..Default::default()
                })
                .unwrap_or(parsed_desc.get_payload_type_for_codec(&Codec {
                    name: "H264".to_string(),
                    clock_rate: 90000,
                    ..Default::default()
                })?);

            let mut ssrc = None;
            for desc in parsed_desc.media_descriptions.iter_mut() {
                if let Some(Some(attribute)) = desc.attribute("ssrc") {
                    ssrc = Some(
                        attribute
                            .split(' ')
                            .collect::<Vec<_>>()
                            .get(0)
                            .context("no ssrc number present")?
                            .parse::<u32>()?,
                    );
                    // desc.attributes.push(Attribute {
                    //     key: "ssrc-group".to_string(),
                    //     value: Some(format!("FID {} {}", ssrc.unwrap(), rtx_ssrc)),
                    // });
                    // desc.attributes.push(Attribute {
                    //     key: "ssrc".to_string(),
                    //     value: Some(format!("{} cname:webrtc-rs", rtx_ssrc)),
                    // });
                    // desc.attributes.push(Attribute {
                    //     key: "ssrc".to_string(),
                    //     value: Some(format!("{} msid:webrtc-rs video", rtx_ssrc)),
                    // });
                    break;
                }
            }

            // we need to rewrite the ssrc attributes to add rtx (TERRIBLE CODE AHEAD)

            // Example from Chrome:
            // a=ssrc-group:FID 1333804302 3809138266
            // a=ssrc:1333804302 cname:F47a0p9cmoj4J7pW
            // a=ssrc:1333804302 msid:3e584163-b8a6-4c09-83b6-b0982690a826 47f5d03a-8b04-405c-b4ff-e4ae21d12bc7
            // a=ssrc:3809138266 cname:F47a0p9cmoj4J7pW
            // a=ssrc:3809138266 msid:3e584163-b8a6-4c09-83b6-b0982690a826 47f5d03a-8b04-405c-b4ff-e4ae21d12bc7

            // println!("Marshalling");
            // let local_desc = parsed_desc.marshal();
            // println!("Marshalled: {}", local_desc);

            let json_str = serde_json::to_string(&local_desc)?;
            let b64 = BASE64_STANDARD.encode(&json_str);
            offer_tx.send(b64).await?;
            println!("Encoded base64, sending...");
            (ulp_pt, h264_pt, ssrc.context("ssrc not found")?)
        } else {
            bail!("generate local_description failed!");
        };

    println!(
        "Got payload types from Lux's SDP:\nH264: {}\nulpfec: {}",
        h264_pt, ulp_pt
    );

    let done_tx_clone = done_tx.clone();
    let (buffer_tx, mut buffer_rx) = tokio::sync::mpsc::unbounded_channel();
    peer_connection.on_ice_connection_state_change(Box::new(
        move |connection_state: RTCIceConnectionState| {
            println!("Connection State has changed {connection_state}");
            if connection_state == RTCIceConnectionState::Failed {
                let _ = done_tx_clone.send(());
            } else if connection_state == RTCIceConnectionState::Connected {
                let done_tx = done_tx_clone.clone();
                let buffer_tx = buffer_tx.clone();
                spawn_blocking(move || {
                    pipeline::start_pipeline(
                        state.bitrate,
                        state.startx,
                        offer.show_mouse,
                        done_tx.subscribe(),
                        buffer_tx,
                        ulp_pt,
                        h264_pt,
                        ssrc,
                    );
                });
            } else if connection_state == RTCIceConnectionState::Disconnected {
                let _ = done_tx_clone.send(());
            } else if connection_state == RTCIceConnectionState::Closed {
                println!("Closing task, connection closed.");
                let _ = done_tx_clone.send(());
            }
            Box::pin(async {})
        },
    ));

    let done_tx4 = done_tx.clone();
    tokio::spawn(async move {
        while let Some(packet) = buffer_rx.recv().await {
            let packet = RTPPacket::unmarshal(&mut packet.as_slice())?;
            if let Err(err) = video_track.write_rtp(&packet).await {
                if Error::ErrClosedPipe == err {
                    // The peerConnection has been closed.
                } else {
                    println!("video_track write err: {err}");
                }
                let _ = done_tx4.send(());
                return Ok::<(), anyhow::Error>(());
            }

            let mut nackable_packets = nackable_packets.lock().await;
            nackable_packets.push_back(packet);
            if nackable_packets.len() >= NACK_CAPACITY {
                nackable_packets.pop_front();
            }
        }

        Ok(())
    });
    done_rx.recv().await.ok();
    println!("Received done signal");
    peer_connection.close().await?;
    Ok(())
}
