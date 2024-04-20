use anyhow::Result;
use base64::prelude::*;
use lazy_static::lazy_static;
use nix::sys::signal::Signal::SIGTERM;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::process::Command;
use std::sync::Mutex;
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

#[derive(Serialize, Deserialize)]
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
    input_tx: tokio::sync::mpsc::UnboundedSender<InputCommand>,
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
    let done_tx1 = done_tx.clone();
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
                    match Command::new("sh")
                        .args(["-c", &format!("{} ! videoconvert ! queue ! x264enc tune=zerolatency speed-preset=superfast bitrate=3000 key-int-max=60 ! video/x-h264, profile=baseline ! rtph264pay pt=96 mtu=1200 ! udpsink host=127.0.0.1 port={}", {
                            if cfg!(target_os = "linux") {
                                "gst-launch-1.0 ximagesrc use-damage=0 ! video/x-raw,width=1366,height=768,framerate=60/1"
                            } else if cfg!(target_os = "macos") {
                                "/Library/Frameworks/GStreamer.framework/Commands/gst-launch-1.0 avfvideosrc capture-screen=true capture-screen-cursor=true ! video/x-raw,width=1280,height=800,framerate=60/1"
                            } else if cfg!(target_os = "windows") {
                                "gst-launch-1.0 gdiscreencapsrc ! video/x-raw,width=1366,height=768,framerate=60/1"
                            } else {
                                unimplemented!()
                            }
                        }, port)])
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
    let input_tx1 = input_tx.clone();
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
    if let Some(local_desc) = peer_connection.local_description().await {
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
        let mut inbound_rtp_packet = vec![0u8; 1200]; // UDP MTU
        while let Ok((n, _)) = listener.recv_from(&mut inbound_rtp_packet).await {
            if let Err(err) = video_track.write(&inbound_rtp_packet[..n]).await {
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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let mut gst_handle = gst_handle.lock().unwrap();
        if let Some(child) = gst_handle.as_mut() {
            if let Some(id) = child.id() {
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(0), SIGTERM).ok();
            }
        }
    }
    Ok(())
}
