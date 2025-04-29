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
    fmt::Display,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use log::*;

use input::{do_input, InputCommand};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use tokio::net::{TcpListener, UdpSocket};

use anyhow::{anyhow, bail, Context, Result};

use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;

use serde::{Deserialize, Serialize};

use str0m::{
    bwe::Bitrate,
    change::SdpOffer,
    rtp::{Extension, ExtensionMap},
    Candidate, Rtc,
};

use base64::prelude::*;

use tokio::{spawn, sync::mpsc::UnboundedSender};

use keys::{Keys, Permissions};

use notify_rust::Notification;

mod input;
pub mod keys;
mod rtc;
mod stun;

pub struct AppError(anyhow::Error);

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

#[derive(Deserialize, Clone)]
struct CreateOffer {
    password: Option<String>,
    key: Option<String>,
    offer: String,
    show_mouse: bool,
}

#[derive(Serialize)]
enum ResponseOffer {
    Offer(String),
    Error(String),
}

fn is_bad_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() ||
            v4.is_link_local()
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() ||
            v6.is_unspecified() ||
            v6.is_unique_local() // Optional: you might want to filter ULA too
        }
    }
}

async fn offer(
    State(state): State<AppState>,
    ConnectInfo(req_addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<CreateOffer>,
) -> Result<(StatusCode, Json<ResponseOffer>), AppError> {
    info!("Received offer");
    let permissions = if let Some(ref password) = payload.password {
        if *password != state.config.password {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(ResponseOffer::Error("Password incorrect.".to_string())),
            ));
        } else {
            Permissions::FullControl
        }
    } else if let Some(ref key) = payload.key {
        if let Some(key_permissions) = state.keys.lock().unwrap().use_key(key.as_str()) {
            info!(
                "Authenticated key {} with permissions: {:?}",
                key, key_permissions
            );
            key_permissions
        } else {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(ResponseOffer::Error("Bad session.".to_string())),
            ));
        }
    } else {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(ResponseOffer::Error(
                "No authentication provided.".to_string(),
            )),
        ));
    };

    let mut exts = ExtensionMap::empty();
    exts.set(1, Extension::AudioLevel);
    exts.set(2, Extension::AbsoluteSendTime);
    exts.set(3, Extension::TransportSequenceNumber);
    exts.set(4, Extension::RtpMid);
    exts.set(5, Extension::PlayoutDelay);
    exts.set(10, Extension::RtpStreamId);
    exts.set(11, Extension::RepairedRtpStreamId);
    exts.set(13, Extension::VideoOrientation);

    let rtc = Rtc::builder()
        .clear_codecs()
        .enable_h264(true)
        .enable_opus(true)
        // needed for zero-latency streaming
        .set_extension_map(exts)
        .set_send_buffer_video(1000)
        .set_stats_interval(Some(Duration::from_secs(1)));

    let mut rtc = if state.config.no_bwe {
        rtc.build()
    } else {
        rtc.enable_bwe(Some(Bitrate::kbps(state.config.target_bitrate as u64)))
            .build()
    };

    let local_ip = stun::get_base("stun.l.google.com:19302").await?;
    let interfaces = NetworkInterface::show()?
        .into_iter()
        .map(|iface| {
            iface
                .addr
                .into_iter()
                .map(move |addr| (iface.name.clone(), addr.ip()))
        })
        .flatten()
        .filter(|(_, addr)| !is_bad_ip(&addr))
        .collect::<Vec<_>>();

    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    let local_socket_addr = SocketAddr::new(local_ip, socket.local_addr()?.port());

    for (_iface, ip) in interfaces.iter() {
        let local_socket_addr = SocketAddr::new(*ip, socket.local_addr()?.port());
        rtc.add_local_candidate(Candidate::host(
            local_socket_addr,
            str0m::net::Protocol::Udp,
        )?);
    }

    info!("Local socket addr: {}", local_socket_addr);

    // add a remote candidate too
    let stun_addr = retry!(stun::get_addr(&socket, "stun.l.google.com:19302").await)?;
    info!("Our public IP is: {stun_addr}");
    rtc.add_local_candidate(Candidate::server_reflexive(
        stun_addr,
        local_socket_addr,
        str0m::net::Protocol::Udp,
    )?);

    let tcp = TcpListener::bind("0.0.0.0:0").await?;
    let tcp_local_socket_addr = SocketAddr::new(local_ip, tcp.local_addr()?.port());
    for (_iface, ip) in interfaces.iter() {
        let local_socket_addr = SocketAddr::new(*ip, tcp.local_addr()?.port());
        rtc.add_local_candidate(Candidate::host(
            local_socket_addr,
            str0m::net::Protocol::Tcp,
        )?);
    }

    let gateway_and_port = if state.config.tcp_upnp {
        if let Ok(gateway) = igd_next::aio::tokio::search_gateway(Default::default()).await {
            info!("Successfully obtained gateway");

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
            info!("TCP server has been opened at {} globally", global_addr);
            rtc.add_local_candidate(Candidate::server_reflexive(
                global_addr,
                tcp_local_socket_addr,
                str0m::net::Protocol::Tcp,
            )?);
            Some((gateway, port))
        } else {
            None
        }
    } else {
        // if tcp-upnp is OFF, we can assume that the
        // server's ports are all open
        rtc.add_local_candidate(Candidate::server_reflexive(
            SocketAddr::new(stun_addr.ip(), tcp.local_addr()?.port()),
            tcp_local_socket_addr,
            str0m::net::Protocol::Tcp,
        )?);
        None
    };

    // Accept an incoming offer from the remote peer
    // and get the corresponding answer.
    let desc_data = BASE64_STANDARD.decode(payload.offer.clone())?;
    let desc_data = std::str::from_utf8(&desc_data)?;
    let their_offer = serde_json::from_str::<SdpOffer>(desc_data)?;
    let answer = rtc.sdp_api().accept_offer(their_offer)?;
    let json_str = serde_json::to_string(&answer)?;
    let b64 = BASE64_STANDARD.encode(&json_str);

    //Notification::new()
        //.summary("Tenebra Server Alert")
        //.icon("network-connect-symbolic")
        //.body(&format!(
            //"Accepted new connection from {}\nPermission level: {:?}",
            //req_addr, permissions
        //))
        //.show()?;

    let state_cloned = state.clone();
    spawn(async move {
        if let Err(e) = rtc::run(
            rtc,
            socket,
            tcp,
            local_socket_addr,
            tcp_local_socket_addr,
            state_cloned,
            payload,
            permissions,
        )
        .await
        {
            info!("Run task exited: {e:?}");
        }

        if let Some((gateway, port)) = gateway_and_port {
            info!("Removing port mapping {}.", port);
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

#[derive(Deserialize, Clone)]
struct CreateKeyRequest {
    password: String,
    view_only: bool,
}

async fn create_key(
    State(state): State<AppState>,
    Json(payload): Json<CreateKeyRequest>,
) -> Result<(StatusCode, String), AppError> {
    if payload.password == state.config.password {
        let key = state.keys.lock().unwrap().create_key(if payload.view_only {
            Permissions::ViewOnly
        } else {
            Permissions::FullControl
        });
        info!("Registering new key: {}", key);
        Ok((StatusCode::OK, key))
    } else {
        Err(AppError(anyhow!("Password incorrect.")))
    }
}

async fn home(State(state): State<AppState>) -> String {
    let mut out = String::new();
    out.push_str("This is a Telewindow server powered by the Tenebra project. https://github.com/UE2020/tenebra/\n\n");
    out.push_str(&format!("{}\n", state.config));
    out.push_str(include_str!("notice.txt"));
    out
}

#[derive(Debug, Clone)]
pub struct AppState {
    input_tx: UnboundedSender<InputCommand>,
    ports: Arc<Mutex<Vec<u16>>>,
    keys: Arc<Mutex<Keys>>,
    config: Config,
}

#[derive(Deserialize, Clone, Debug)]
struct Config {
    target_bitrate: u32,
    startx: u32,
    #[serde(default)]
    starty: u32,
    endx: Option<u32>,
    endy: Option<u32>,
    port: u16,
    password: String,
    sound_forwarding: bool,
    #[serde(alias = "hwencode")]
    vaapi: bool,
    vapostproc: bool,
    no_bwe: bool,
    full_chroma: bool,
    tcp_upnp: bool,
    #[serde(default = "default_vbv_buf_capacity")]
    vbv_buf_capacity: u32,
    cert: PathBuf,
    key: PathBuf,
}

impl Display for Config {
    #[rustfmt::skip]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "Server configuration")?;
        writeln!(f, "\tTarget bitrate:                    {} Kbit/s", self.target_bitrate)?;
        writeln!(f, "\tStart x-coordinate:                {}", self.startx)?;
        writeln!(f, "\tStart y-coordinate:                {}", self.starty)?;
        writeln!(f, "\tEnd x-coordinate:                  {:?}", self.endx)?;
        writeln!(f, "\tEnd y-coordinate:                  {:?}", self.endy)?;
        writeln!(f, "\tPort:                              {}", self.port)?;
        writeln!(f, "\tSound forwarding:                  {}", bool_to_str(self.sound_forwarding))?;
        writeln!(f, "\tHardware accelerated encoding:     {}", bool_to_str(self.vaapi))?;
        writeln!(f, "\tVA-API format conversion:          {}", bool_to_str(self.vapostproc))?;
        writeln!(f, "\tBandwidth estimation:              {}", bool_to_str(!self.no_bwe))?;
        writeln!(f, "\tFull color encoding:               {}", bool_to_str(self.full_chroma))?;
        writeln!(f, "\tAutomatic ICE-TCP UPnP forwarding: {}", bool_to_str(self.tcp_upnp))?;
        writeln!(f, "\tVBV Buffer capacity:               {} ms", self.vbv_buf_capacity)?;

        Ok(())
    }
}

fn bool_to_str(b: bool) -> &'static str {
    match b {
        true => "on",
        false => "off",
    }
}

fn default_vbv_buf_capacity() -> u32 {
    120
}

#[tokio::main]
async fn main() -> Result<()> {
    // WinCrypto simplifies build significantly on Windows
    #[cfg(target_os = "windows")]
    str0m::config::CryptoProvider::WinCrypto.install_process_default();

    // check if we're behind symmetric NAT
    if stun::is_symmetric_nat().await? {
        bail!("You are behind a symmetric NAT. This configuration prevents STUN binding requests from establishing a proper connection. Please adjust your network settings or consult your network administrator.");
    }

    pretty_env_logger::init_timed();

    // Initialize GStreamer
    gstreamer::init().unwrap();

    // get the config path
    let mut config_path = dirs::config_dir()
        .context("Failed to find config directory")?
        .join("tenebra");
    std::fs::create_dir_all(&config_path)?;
    config_path.push("config.toml");
    if !config_path.exists() {
        std::fs::write(&config_path, include_bytes!("default.toml"))?;
    }

    // read the config
    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;

    println!("{}", config);

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<InputCommand>();
    let ports = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/", get(home))
        .route("/create_key", post(create_key))
        .route("/offer", post(offer))
        .layer(tower_http::cors::CorsLayer::very_permissive())
        .with_state(AppState {
            input_tx: tx,
            config: config.clone(),
            keys: Arc::new(Mutex::new(Keys::new())),
            ports: ports.clone(),
        });

    let tls_config =
        RustlsConfig::from_pem(std::fs::read(&config.cert)?, std::fs::read(&config.key)?).await?;

    spawn(async move {
        axum_server::bind_rustls(SocketAddr::from(([0, 0, 0, 0], config.port)), tls_config)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    });

    println!("Tenebra is listening on port {}.", config.port);

    if config.tcp_upnp {
        match igd_next::aio::tokio::search_gateway(Default::default()).await {
            Ok(gateway) => {
                use tokio::signal::ctrl_c;
                spawn(async move {
                    #[cfg(target_family = "unix")]
                    let mut sigterm_stream =
                        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                            .unwrap();

                    #[cfg(target_family = "unix")]
                    let mut sighup_stream =
                        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                            .unwrap();

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
            Err(e) => error!("Error obtaining UPnP gateway: {}", e),
        }
    }

    #[cfg(target_os = "linux")]
    tokio::task::spawn_blocking(move || do_input(rx, config.startx, config.starty)).await??;

    #[cfg(not(target_os = "linux"))]
    tokio::task::block_in_place(move || do_input(rx, config.startx, config.starty))?;

    Ok(())
}
