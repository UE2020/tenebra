use anyhow::Result;
use base64::prelude::*;
use bytes::{BufMut, Bytes};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use webrtc::rtp::extension::HeaderExtension;
use webrtc::rtp::packet::Packet;
use webrtc::util::{Marshal, MarshalSize, Unmarshal};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::process::Command;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::Error;

use crate::AppState;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputCommand {
    pub r#type: String,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub button: Option<u8>,
    pub key: Option<String>,
}

lazy_static! {
    static ref PORT: Arc<Mutex<usize>> = Arc::new(Mutex::new(6000));
}

pub async fn start_video_streaming(
    offer: String,
    offer_tx: tokio::sync::mpsc::Sender<String>,
    state: AppState,
) -> Result<(), anyhow::Error> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut m)?;
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
            mime_type: MIME_TYPE_H264.to_owned(),
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
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
    {
        *state.kill_switch.lock().unwrap() = Some(done_tx.clone());    
    }
    let done_tx1: tokio::sync::mpsc::Sender<()> = done_tx.clone();
    let gst_handle = Arc::new(Mutex::new(None));
    let port = {
        let mut port = PORT.lock().unwrap();
        *port += 1;
        *port
    };
    dbg!(port);
    let gst_handle_clone = gst_handle.clone();
    peer_connection.on_ice_connection_state_change(Box::new(
        move |connection_state: RTCIceConnectionState| {
            println!("Connection State has changed {connection_state}");
            if connection_state == RTCIceConnectionState::Failed {
                let _ = done_tx1.try_send(());
            } else if connection_state == RTCIceConnectionState::Connected {
                    // TODO: remove startx=0
                    let command = if cfg!(target_os = "linux") {
                        "gst-launch-1.0"
                    } else if cfg!(target_os = "macos") {
                        "/Library/Frameworks/GStreamer.framework/Commands/gst-launch-1.0"
                    } else {
                        r"C:\gstreamer\1.0\msvc_x86_64\bin\gst-launch-1.0.exe"
                    };
                    let args = &format!("{} ! queue ! videoconvert n-threads=4 ! video/x-raw,format=NV12 ! queue ! x264enc qos=true threads=4 aud=true b-adapt=false bframes=0 insert-vui=true rc-lookahead=0 vbv-buf-capacity=120 sliced-threads=true byte-stream=true pass=cbr speed-preset=veryfast tune=zerolatency bitrate={} ! video/x-h264,profile=baseline,stream-format=byte-stream ! queue ! rtph264pay mtu=1000 aggregate-mode=zero-latency config-interval=-1 ! application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=97,rtcp-fb-nack-pli=true,rtcp-fb-ccm-fir=true,rtcp-fb-x-gstreamer-fir-as-repair=true ! queue ! udpsink host=127.0.0.1 port={}", {
                        if cfg!(target_os = "linux") {
                            format!("ximagesrc use-damage=0 startx={} blocksize=16384 remote=true ! video/x-raw,width={},height={},framerate=60/1", state.startx, state.width, state.height)
                        } else if cfg!(target_os = "macos") {
                            format!("avfvideosrc capture-screen=true capture-screen-cursor=true ! video/x-raw,width={},height={},framerate=60/1", state.width, state.height)
                        } else if cfg!(target_os = "windows") {
                            format!("d3d11screencapturesrc show-cursor=true capture-api=wgc ! video/x-raw,width={},height={},framerate=60/1", state.width, state.height)
                        } else {
                            unimplemented!()
                        }
                    }, state.bitrate, port);
                    println!("Using command: {}", args);
                    let args = shell_words::split(args).unwrap();
                    match Command::new(command)
                        .args(args)
                        .kill_on_drop(true)
                        .spawn() {
                            Ok(child) => {
                                {
                                    *gst_handle_clone.lock().unwrap() = Some(child);
                                }
                            },
                            Err(_) => {
                                let _ = done_tx1.try_send(());
                            }
                        }
            } else if connection_state == RTCIceConnectionState::Disconnected {
                let _ = done_tx1.try_send(());
            } else if connection_state == RTCIceConnectionState::Closed {
                println!("Closing task, connection closed.");
                let _ = done_tx1.try_send(());
            }
            Box::pin(async {})
        },
    ));
    let done_tx2 = done_tx.clone();
    peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        println!("Peer Connection State has changed: {s}");
        if s == RTCPeerConnectionState::Failed {
            // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
            // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
            // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
            println!("Peer Connection has gone to failed exiting: Done forwarding");
            let _ = done_tx2.try_send(());
        }
        Box::pin(async {})
    }));
    let done_tx3 = done_tx.clone();
    let input_tx1 = state.input_tx.clone();
    peer_connection.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        let d_label = d.label().to_owned();
        let d_id = d.id();
        println!("New DataChannel {d_label} {d_id}");

        // Register channel opening handling
        let done_tx4 = done_tx3.clone();
        let done_tx5 = done_tx3.clone();
        let input_tx2 = input_tx1.clone();
        Box::pin(async move {
            let d_label2 = d_label.clone();
            let d_id2 = d_id;
            d.on_close(Box::new(move || {
                println!("Data channel closed");
                let _ = done_tx4.try_send(());
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
                        let _ = done_tx5.try_send(());
                        return Box::pin(async {});
                    }
                };

                let _ = input_tx2.send(cmd);

                Box::pin(async {})
            }));
        })
    }));
    let desc_data = BASE64_STANDARD.decode(offer)?;
    let desc_data = std::str::from_utf8(&desc_data)?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(answer).await?;
    let _ = gather_complete.recv().await;
    if let Some(mut local_desc) = peer_connection.local_description().await {
        // add playout delay since we support it
        local_desc.sdp = local_desc.sdp.replace("a=extmap:4 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01\r\n", "a=extmap:5 http://www.webrtc.org/experiments/rtp-hdrext/playout-delay\r\n");
        // a=extmap:5 http://www.webrtc.org/experiments/rtp-hdrext/playout-delay
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = BASE64_STANDARD.encode(&json_str);
        offer_tx.send(b64).await?;
        println!("Encoded base64, sending...");
    } else {
        println!("generate local_description failed!");
    }
    let listener = UdpSocket::bind(format!("127.0.0.1:{}", port)).await?;
    let done_tx4 = done_tx.clone();
    tokio::spawn(async move {
        let mut inbound_rtp_packet = vec![0u8; 1000]; // UDP MTU
        while let Ok((n, _)) = listener.recv_from(&mut inbound_rtp_packet).await {
            let mut data = &inbound_rtp_packet[..n];
            let mut packet = Packet::unmarshal(&mut data).unwrap();
            // http://www.webrtc.org/experiments/rtp-hdrext/playout-delay
            // 0                   1                   2                   3
            // 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
            // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
            // |  ID   | len=2 |       MIN delay       |       MAX delay       |
            // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
            // we want all zeros, so a zeroed payload is fine.
            // (???) from wireshark with Xbox Cloud
            // RFC 5285 Header Extension (One-Byte Header)
            // Identifier: 2
            // Length: 3
            // Extension Data: 49072b
            //packet.header.set_extension(1, Bytes::copy_from_slice(&[0xff])).unwrap(); X
            //packet.header.set_extension(2, Bytes::copy_from_slice(&[0x49, 0x07, 0x2b])).unwrap();
            //packet.header.set_extension(3, Bytes::copy_from_slice(&counter.to_be_bytes())).unwrap();
            //packet.header.set_extension(4, Bytes::copy_from_slice(&[0x31])).unwrap();
            packet.header.set_extension(5, Bytes::copy_from_slice(&[0x00, 0x00, 0x00])).unwrap();
            if let Err(err) = video_track.write_rtp(&packet).await {
                if Error::ErrClosedPipe == err {
                    // The peerConnection has been closed.
                } else {
                    println!("video_track write err: {err}");
                }
                let _ = done_tx4.try_send(());
                return;
            }
        }
    });
    done_rx.recv().await;
    println!("Task close finished.");
    peer_connection.close().await?;
    println!("Function returning, process will be dropped shortly.");
    if let Some(process) = gst_handle.lock().unwrap().as_mut() {
        process.start_kill().ok();
    }
    Ok(())
}

pub struct PlayoutDelay {
    pub min: u8,
    pub max: u8
}

impl MarshalSize for PlayoutDelay {
    fn marshal_size(&self) -> usize {
        3
    }
}

impl Marshal for PlayoutDelay {
    fn marshal_to(&self, mut buf: &mut [u8]) -> webrtc::util::Result<usize> {
        if buf.remaining_mut() < 3 {
            return Err(webrtc::rtp::Error::ErrBufferTooSmall.into());
        }

        Ok(3)
    }
}