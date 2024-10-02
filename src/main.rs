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

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use input::{do_input, InputCommand};
use local_ip_address::local_ip;
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::mpsc::unbounded_channel,
};

use anyhow::{bail, Result};

use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum_server::tls_openssl::OpenSSLConfig;

use serde::{Deserialize, Serialize};

use str0m::{
    bwe::Bitrate,
    change::SdpOffer,
    rtp::{Extension, ExtensionMap},
    Candidate, Rtc,
};

use base64::prelude::*;

use tokio::{spawn, sync::mpsc::UnboundedSender};

mod input;
mod rtc;
mod stun;

#[derive(Deserialize, Clone)]
struct CreateOffer {
    password: String,
    offer: String,
    show_mouse: bool,
}

#[derive(Serialize)]
enum ResponseOffer {
    Offer(String),
    Error(String),
}

pub struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {:?}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[axum_macros::debug_handler]
async fn offer(
    State(state): State<AppState>,
    Json(payload): Json<CreateOffer>,
) -> Result<(StatusCode, Json<ResponseOffer>), AppError> {
    println!("Received offer");
    if payload.password != state.password {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(ResponseOffer::Error("Password incorrect.".to_string())),
        ));
    }

    let mut exts = ExtensionMap::empty();
    exts.set(1, Extension::AudioLevel);
    exts.set(2, Extension::AbsoluteSendTime);
    exts.set(3, Extension::TransportSequenceNumber);
    exts.set(4, Extension::RtpMid);
    exts.set(5, Extension::PlayoutDelay);
    exts.set(10, Extension::RtpStreamId);
    exts.set(11, Extension::RepairedRtpStreamId);
    exts.set(13, Extension::VideoOrientation);

    // Instantiate a new Rtc instance.
    let mut rtc = Rtc::builder()
        .clear_codecs()
        .enable_h264(true)
        // needed for zero-latency streaming
        .set_extension_map(exts)
        .set_send_buffer_video(1000)
        .enable_bwe(Some(Bitrate::kbps(3000)))
        .enable_bwe(Some(Bitrate::kbps(4000)))
        .set_stats_interval(Some(Duration::from_secs(1)))
        .build();

    let local_ip = local_ip()?;

    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    let local_socket_addr = SocketAddr::new(local_ip, socket.local_addr()?.port());
    rtc.add_local_candidate(Candidate::host(
        local_socket_addr,
        str0m::net::Protocol::Udp,
    )?);

    println!("Local socket addr: {}", local_socket_addr);

    // add a remote candidate too
    let stun_addr = retry!(stun::get_addr(&socket, "stun.l.google.com:19302").await)?;
    println!("Our public IP is: {stun_addr}");
    rtc.add_local_candidate(Candidate::server_reflexive(
        stun_addr,
        local_socket_addr,
        str0m::net::Protocol::Udp,
    )?);

    let tcp = TcpListener::bind("0.0.0.0:0").await?;
    let tcp_local_socket_addr = SocketAddr::new(local_ip, tcp.local_addr()?.port());
    rtc.add_local_candidate(Candidate::host(
        tcp_local_socket_addr,
        str0m::net::Protocol::Tcp,
    )?);

    #[cfg(feature = "tcp-upnp")]
    let gateway_and_port =
        if let Ok(gateway) = igd_next::aio::tokio::search_gateway(Default::default()).await {
            println!("Successfully obtained gateway");

            let port = gateway
                .add_any_port(
                    igd_next::PortMappingProtocol::TCP,
                    tcp_local_socket_addr,
                    0,
                    "ICE-TCP port",
                )
                .await?;

            state.ports.lock().unwrap().push(port);

            let global_ip = gateway.get_external_ip().await.unwrap();
            let global_addr = SocketAddr::new(global_ip, port);
            println!("TCP server has been opened at {} globally", global_addr);
            rtc.add_local_candidate(Candidate::server_reflexive(
                global_addr,
                tcp_local_socket_addr,
                str0m::net::Protocol::Tcp,
            )?);
            Some((gateway, port))
        } else {
            None
        };

    // if tcp-upnp is OFF, we can assume that the
    // server's ports are all open
    #[cfg(not(feature = "tcp-upnp"))]
    rtc.add_local_candidate(Candidate::server_reflexive(
        SocketAddr::new(stun_addr.ip(), tcp.local_addr()?.port()),
        tcp_local_socket_addr,
        str0m::net::Protocol::Tcp,
    )?);

    // Accept an incoming offer from the remote peer
    // and get the corresponding answer.
    let desc_data = BASE64_STANDARD.decode(payload.offer.clone())?;
    let desc_data = std::str::from_utf8(&desc_data)?;
    let their_offer = serde_json::from_str::<SdpOffer>(&desc_data)?;
    let answer = rtc.sdp_api().accept_offer(their_offer).unwrap();
    let json_str = serde_json::to_string(&answer)?;
    let b64 = BASE64_STANDARD.encode(&json_str);

    println!("Killing last session");
    // kill last session and
    let kill_rx = {
        let mut sender = state.kill_switch.lock().unwrap();
        if let Some(sender) = sender.as_mut() {
            sender.send(()).ok();
        };
        let (kill_tx, kill_rx) = unbounded_channel();
        *sender = Some(kill_tx);
        kill_rx
    };

    spawn(async move {
        if let Err(e) = rtc::run(
            rtc,
            socket,
            tcp,
            local_socket_addr,
            tcp_local_socket_addr,
            state.clone(),
            payload,
            kill_rx,
        )
        .await
        {
            eprintln!("Run task exited: {e:?}");
        }

        #[cfg(feature = "tcp-upnp")]
        if let Some((gateway, port)) = gateway_and_port {
            println!("Removing port mapping {}.", port);
            gateway
                .remove_port(igd_next::PortMappingProtocol::TCP, port)
                .await
                .ok();

            // remove from port list
            let mut ports = state.ports.lock().unwrap();
            let index = ports
                .iter()
                .position(|some_port| *some_port == port)
                .unwrap();
            ports.remove(index);
        }
    });

    Ok((StatusCode::OK, Json(ResponseOffer::Offer(b64))))
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    version: String,
    plugins: Vec<String>,
    cpu_names: Vec<String>,
}

struct HtmlTemplate<T>(T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template. Error: {err}"),
            )
                .into_response(),
        }
    }
}
async fn home() -> impl IntoResponse {
    let registry = gstreamer::Registry::get();
    let plugins = registry
        .plugins()
        .into_iter()
        .map(|plugin| plugin.plugin_name().to_string())
        .collect::<Vec<_>>();
    use sysinfo::{CpuRefreshKind, RefreshKind, System};
    let s = System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::everything()));
    let cpu_names = s
        .cpus()
        .into_iter()
        .map(|cpu| format!("{}: {}", cpu.name(), cpu.brand()))
        .collect::<Vec<_>>();
    let template = HomeTemplate {
        version: gstreamer::version_string().to_string(),
        plugins,
        cpu_names,
    };
    HtmlTemplate(template)
}

#[derive(Debug, Clone)]
pub struct AppState {
    input_tx: UnboundedSender<InputCommand>,
    kill_switch: Arc<Mutex<Option<UnboundedSender<()>>>>,
    bitrate: u32,
    startx: u32,
    password: String,
    ports: Arc<Mutex<Vec<u16>>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // check if we're behind symmetric NAT
    if stun::is_symmetric_nat().await? {
        bail!("You are behind a symmetric NAT. This configuration prevents STUN binding requests from establishing a proper connection. Please adjust your network settings or consult your network administrator.");
    }

    // Initialize GStreamer
    gstreamer::init().unwrap();

    let args = std::env::args().collect::<Vec<_>>();
    let password = &args[1];
    let port = args[2]
        .parse::<u32>()
        .expect("port should be passed as a numerical argument");
    let bitrate = args
        .get(3)
        .unwrap_or(&"4000".to_owned())
        .parse::<u32>()
        .expect("bitrate should be passed as a numerical argument");
    let startx = args
        .get(4)
        .unwrap_or(&"0".to_owned())
        .parse::<u32>()
        .expect("startx should be passed as a numerical argument");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<InputCommand>();
    let ports = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/", get(home))
        .route("/offer", post(offer))
        .layer(tower_http::cors::CorsLayer::very_permissive())
        .with_state(AppState {
            input_tx: tx,
            kill_switch: Arc::new(Mutex::new(None)),
            bitrate,
            startx,
            password: password.to_string(),
            ports: ports.clone(),
        });

    let config =
        OpenSSLConfig::from_pem(include_bytes!("../cert.pem"), include_bytes!("../key.pem"))?;

    spawn(async move {
        axum_server::bind_openssl(SocketAddr::from(([0, 0, 0, 0], port as u16)), config)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    println!("Tenebra is listening on port {}.", port);

    // We can try to forward the server with UPnP
    match igd_next::aio::tokio::search_gateway(Default::default()).await {
        Ok(gateway) => {
            use tokio::signal::ctrl_c;
            #[cfg(feature = "upnp")]
            let local_addr = SocketAddr::new(local_ip()?, port as u16);
            #[cfg(feature = "upnp")]
            match gateway
                .add_any_port(
                    igd_next::PortMappingProtocol::TCP,
                    local_addr,
                    0,
                    "Telewindow server",
                )
                .await
            {
                Err(ref err) => {
                    println!("There was an error! {err}");
                }
                Ok(port) => {
                    ports.lock().unwrap().push(port);
                    let global_ip = gateway.get_external_ip().await.unwrap();
                    let global_addr = SocketAddr::new(global_ip, port);
                    println!(
                        "The tenebra service has been portforwarded.\nThe external address is {global_addr}"
                    );
                }
            }

            spawn(async move {
                #[cfg(target_family = "unix")]
                let mut sigterm_stream =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .unwrap();

                #[cfg(target_family = "unix")]
                let mut sighup_stream =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).unwrap();

                #[cfg(target_family = "unix")]
                tokio::select! {
                    _ = ctrl_c() => {},
                    _ = sigterm_stream.recv() => {},
                    _ = sighup_stream.recv() => {},
                }

                #[cfg(not(target_family = "unix"))]
                ctrl_c().await.unwrap();

                let ports = ports.lock().unwrap().clone();

                println!();
                for port in ports {
                    gateway
                        .remove_port(igd_next::PortMappingProtocol::TCP, port)
                        .await
                        .ok();
                    println!("Port mapping {port} removed. Exiting...");
                }

                std::process::exit(0);
            });
        }
        Err(e) => println!("Error obtaining UPnP gateway: {}", e),
    }

    #[cfg(target_os = "linux")]
    tokio::task::spawn_blocking(move || do_input(rx, startx)).await??;

    #[cfg(not(target_os = "linux"))]
    tokio::task::block_in_place(move || do_input(rx, startx))?;

    Ok(())
}
