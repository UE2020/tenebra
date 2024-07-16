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

use anyhow::bail;
use anyhow::Result;

use base64::prelude::*;

use serde::{Deserialize, Serialize};
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::sdp::util::Codec;

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
    let mut registry = Registry::new();
    // nack MAY break fec. more research needed
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
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
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
    let desc_data = BASE64_STANDARD.decode(offer.offer)?;
    let desc_data = std::str::from_utf8(&desc_data)?;
    let our_offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    peer_connection.set_remote_description(our_offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(answer).await?;
    let _ = gather_complete.recv().await;
    let (ulp_pt, h264_pt) = if let Some(local_desc) = peer_connection.local_description().await {
        let parsed_desc = local_desc.unmarshal()?;
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
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = BASE64_STANDARD.encode(&json_str);
        offer_tx.send(b64).await?;
        println!("Encoded base64, sending...");
        (ulp_pt, h264_pt)
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
            if let Err(err) = video_track.write(&packet).await {
                if Error::ErrClosedPipe == err {
                    // The peerConnection has been closed.
                } else {
                    println!("video_track write err: {err}");
                }
                let _ = done_tx4.send(());
                return;
            }
        }
    });
    done_rx.recv().await.ok();
    println!("Received done signal");
    peer_connection.close().await?;
    Ok(())
}
