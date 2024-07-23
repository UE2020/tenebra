use anyhow::anyhow;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::thread::spawn;
use std::time::Instant;
use str0m::bwe::Bitrate;
use str0m::bwe::BweKind;
use str0m::channel::ChannelData;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::unbounded_channel;

use anyhow::Context;
use str0m::media::Frequency;
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

pub enum GStreamerControlMessage {
    Stop,
    RequestKeyFrame,
    Bitrate(u32),
}

struct GStreamerInstance {
    buffer_rx: UnboundedReceiver<(Vec<u8>, u64)>,
    control_tx: UnboundedSender<GStreamerControlMessage>,
    media: MediaAdded,
    start: Instant,
}

pub async fn run(
    mut rtc: Rtc,
    socket: UdpSocket,
    local_socket_addr: SocketAddr,
    state: AppState,
    offer: CreateOffer,
    mut kill_rx: UnboundedReceiver<()>,
) -> anyhow::Result<()> {
    let mut buf = Vec::new();

    let mut gstreamers: Vec<GStreamerInstance> = vec![];

    let ret = loop {
        if kill_rx.try_recv().is_ok() {
            break Err(anyhow!("task killed from the kill_tx"));
        }

        for gstreamer in gstreamers.iter_mut() {
            let buf = gstreamer.buffer_rx.try_recv();

            if let Ok((buf, _)) = buf {
                let writer = rtc
                    .writer(gstreamer.media.mid)
                    .context("couldn't get rtc writer")?
                    .playout_delay(MediaTime::ZERO, MediaTime::ZERO);
                let pt = writer.payload_params().nth(0).unwrap().pt();
                let now = Instant::now();
                let mt: MediaTime = (now - gstreamer.start).into();
                let mt = mt.rebase(Frequency::NINETY_KHZ);
                writer.write(pt, now, mt, buf)?;
            }
        }

        // Poll output until we get a timeout. The timeout means we are either awaiting UDP socket input
        // or the timeout to happen.
        let output = rtc.poll_output()?;
        let time = match output {
            Output::Timeout(v) => v,

            Output::Transmit(v) => {
                socket.send_to(&v.contents, v.destination).await?;
                continue;
            }

            Output::Event(v) => {
                //println!("Received RTP event: {:?}", v);
                match v {
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        break Ok(());
                    }
                    Event::MediaAdded(media_added) => {
                        // rtc.direct_api()
                        //     .stream_tx_by_mid(media_added.mid, None)
                        //     .unwrap()
                        //     .set_unpaced(true);
                        let (control_tx, control_rx) =
                            unbounded_channel::<GStreamerControlMessage>();
                        let (buffer_tx, buffer_rx) = unbounded_channel();
                        gstreamers.push(GStreamerInstance {
                            buffer_rx,
                            control_tx,
                            media: media_added,
                            start: Instant::now(),
                        });
                        spawn(move || {
                            pipeline::start_pipeline(
                                state.bitrate,
                                state.startx,
                                offer.show_mouse,
                                control_rx,
                                buffer_tx,
                            );
                        });
                    }
                    Event::PeerStats(stats) => {
                        println!("Peer stats: {:?}", stats);
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
                        let bwe = (bitrate.as_u64() / 1000).min(10000) as u32;
                        for gstreamer in gstreamers.iter_mut() {
                            gstreamer
                                .control_tx
                                .send(GStreamerControlMessage::Bitrate(bwe))?;
                        }
                        rtc.bwe().set_current_bitrate(bitrate);
                        rtc.bwe()
                            .set_desired_bitrate(Bitrate::kbps(state.bitrate as u64));
                    }
                    Event::ChannelData(ChannelData { data, .. }) => {
                        let msg_str = String::from_utf8(data)?;
                        let cmd: InputCommand = serde_json::from_str(&msg_str)?;
                        state.input_tx.send(cmd)?;
                    }
                    Event::IceConnectionStateChange(IceConnectionState::Connected) => {
                        println!("ICE Connection state is now CONNECTED. Waiting for media to be added...");
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

        let input = match tokio::time::timeout(timeout, socket.recv_from(&mut buf)).await {
            Ok(Ok((n, source))) => {
                // UDP data received.
                Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Udp,
                        source,
                        destination: SocketAddr::new(
                            local_socket_addr.ip(),
                            socket.local_addr()?.port(),
                        ),
                        contents: (&buf[..n]).try_into()?,
                    },
                )
            }
            Ok(Err(e)) => match e.kind() {
                ErrorKind::ConnectionReset => continue,
                _ => {
                    println!("[TransportWebrtc] network error {:?}", e);
                    break Err(e.into());
                }
            },
            Err(_e) => {
                // Expected error for set_read_timeout().
                // One for windows, one for the rest.
                Input::Timeout(Instant::now())
            }
        };

        // Input is either a Timeout or Receive of data. Both drive the state forward.
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
