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
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;

use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

use enigo::{
    Button, Coordinate,
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};

use serde::{Deserialize, Serialize};

use streaming::InputCommand;

use tokio::{
    spawn,
    sync::{broadcast::Sender, mpsc::UnboundedSender},
};

mod streaming;

#[derive(Deserialize)]
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
            format!("Something went wrong: {}", self.0),
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
    println!("Killing last session");
    // kill last session
    {
        let mut sender = state.kill_switch.lock().unwrap();
        if let Some(sender) = sender.as_mut() {
            sender.send(()).ok();
        };
        *sender = None;
    }
    println!("Spawning!");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);
    let task = tokio::spawn(streaming::start_video_streaming(payload, tx, state));
    tokio::select! {
        val = rx.recv() => {
            Ok((StatusCode::OK, Json(ResponseOffer::Offer(val.unwrap()))))
        }
        _ = task => {
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ResponseOffer::Error("Internal error".to_string())),
            ))
        }
    }
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    version: String,
    plugins: Vec<String>,
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
    let template = HomeTemplate {
        version: gstreamer::version_string().to_string(),
        plugins,
    };
    HtmlTemplate(template)
}

#[derive(Debug, Clone)]
pub struct AppState {
    input_tx: UnboundedSender<InputCommand>,
    kill_switch: Arc<Mutex<Option<Sender<()>>>>,
    bitrate: u32,
    startx: u32,
    password: String,
}

#[tokio::main]
async fn main() -> Result<()> {
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
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<InputCommand>();
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
        });
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();
    spawn(async { axum::serve(listener, app).await });

    let mut enigo = Enigo::new(&Settings {
        linux_delay: 1,
        ..Default::default()
    })
    .unwrap();

    let mut last_capslock = Instant::now();

    while let Some(msg) = rx.recv().await {
        match msg {
            InputCommand {
                r#type,
                x: Some(x),
                y: Some(y),
                ..
            } => match r#type.as_str() {
                "mousemove" => enigo
                    .move_mouse(x as i32, y as i32, Coordinate::Rel)
                    .unwrap(),
                "mousemoveabs" => enigo
                    .move_mouse(x as i32 + startx as i32, y as i32, Coordinate::Abs)
                    .unwrap(),
                "wheel" => {
                    enigo
                        .scroll((x / 120.0) as i32, enigo::Axis::Horizontal)
                        .unwrap();
                    enigo
                        .scroll((y / 120.0) as i32, enigo::Axis::Vertical)
                        .unwrap();
                }
                _ => {}
            },
            InputCommand {
                r#type,
                button: Some(button),
                ..
            } => {
                enigo
                    .button(
                        match button {
                            0 => Button::Left,
                            1 => Button::Middle,
                            2 => Button::Right,
                            _ => continue,
                        },
                        match r#type.as_str() {
                            "mousedown" => Press,
                            "mouseup" => Release,
                            _ => continue,
                        },
                    )
                    .unwrap();
            }
            InputCommand {
                r#type,
                key: Some(key),
                ..
            } => {
                let key = match key.as_str() {
                    "Escape" => Key::Escape,
                    "Digit1" => Key::Unicode('1'),
                    "Digit2" => Key::Unicode('2'),
                    "Digit3" => Key::Unicode('3'),
                    "Digit4" => Key::Unicode('4'),
                    "Digit5" => Key::Unicode('5'),
                    "Digit6" => Key::Unicode('6'),
                    "Digit7" => Key::Unicode('7'),
                    "Digit8" => Key::Unicode('8'),
                    "Digit9" => Key::Unicode('9'),
                    "Digit0" => Key::Unicode('0'),
                    "Minus" => Key::Unicode('-'),
                    "Equal" => Key::Unicode('='),
                    "Backspace" => Key::Backspace,
                    "Tab" => Key::Tab,
                    "KeyQ" => Key::Unicode('q'),
                    "KeyW" => Key::Unicode('w'),
                    "KeyE" => Key::Unicode('e'),
                    "KeyR" => Key::Unicode('r'),
                    "KeyT" => Key::Unicode('t'),
                    "KeyY" => Key::Unicode('y'),
                    "KeyU" => Key::Unicode('u'),
                    "KeyI" => Key::Unicode('i'),
                    "KeyO" => Key::Unicode('o'),
                    "KeyP" => Key::Unicode('p'),
                    "BracketLeft" => Key::Unicode('['),
                    "BracketRight" => Key::Unicode(']'),
                    "Enter" => Key::Return,
                    "ControlLeft" => Key::Control,
                    "KeyA" => Key::Unicode('a'),
                    "KeyS" => Key::Unicode('s'),
                    "KeyD" => Key::Unicode('d'),
                    "KeyF" => Key::Unicode('f'),
                    "KeyG" => Key::Unicode('g'),
                    "KeyH" => Key::Unicode('h'),
                    "KeyJ" => Key::Unicode('j'),
                    "KeyK" => Key::Unicode('k'),
                    "KeyL" => Key::Unicode('l'),
                    "Semicolon" => Key::Unicode(';'),
                    "Quote" => Key::Unicode('\''),
                    "Backquote" => Key::Unicode('`'),
                    "ShiftLeft" => Key::Shift,
                    "Backslash" => Key::Unicode('\\'),
                    "KeyZ" => Key::Unicode('z'),
                    "KeyX" => Key::Unicode('x'),
                    "KeyC" => Key::Unicode('c'),
                    "KeyV" => Key::Unicode('v'),
                    "KeyB" => Key::Unicode('b'),
                    "KeyN" => Key::Unicode('n'),
                    "KeyM" => Key::Unicode('m'),
                    "Comma" => Key::Unicode(','),
                    "Period" => Key::Unicode('.'),
                    "Slash" => Key::Unicode('/'),
                    "ShiftRight" => Key::Shift,
                    "NumpadMultiply" => Key::Unicode('*'),
                    "AltLeft" => Key::Alt,
                    "Space" => Key::Space,
                    "CapsLock" => Key::CapsLock,
                    "F1" => Key::F1,
                    "F2" => Key::F2,
                    "F3" => Key::F3,
                    "F4" => Key::F4,
                    "F5" => Key::F5,
                    "F6" => Key::F6,
                    "F7" => Key::F7,
                    "F8" => Key::F8,
                    "F9" => Key::F9,
                    "F10" => Key::F10,
                    #[cfg(not(target_os = "macos"))]
                    "NumLock" => Key::Numlock,
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    "ScrollLock" => Key::ScrollLock,
                    "Numpad7" => Key::Unicode('7'),
                    "Numpad8" => Key::Unicode('8'),
                    "Numpad9" => Key::Unicode('9'),
                    "NumpadSubtract" => Key::Unicode('-'),
                    "Numpad4" => Key::Unicode('4'),
                    "Numpad5" => Key::Unicode('5'),
                    "Numpad6" => Key::Unicode('6'),
                    "NumpadAdd" => Key::Unicode('+'),
                    "Numpad1" => Key::Unicode('1'),
                    "Numpad2" => Key::Unicode('2'),
                    "Numpad3" => Key::Unicode('3'),
                    "Numpad0" => Key::Unicode('0'),
                    "NumpadDecimal" => Key::Unicode('.'),
                    "IntlBackslash" => Key::Unicode('\\'),
                    "IntlRo" => Key::Unicode('\\'), // Assuming IntlRo is the Korean won symbol
                    "IntlYen" => Key::Unicode('¥'), // Assuming IntlYen is the Japanese yen symbol
                    "F11" => Key::F11,
                    "F12" => Key::F12,
                    "NumpadEnter" => Key::Return,
                    "ControlRight" => Key::Control,
                    "NumpadDivide" => Key::Unicode('/'),
                    #[cfg(not(target_os = "macos"))]
                    "PrintScreen" => Key::Print,
                    "AltRight" => Key::Alt,
                    "Home" => Key::Home,
                    "ArrowUp" => Key::UpArrow,
                    "PageUp" => Key::PageUp,
                    "ArrowLeft" => Key::LeftArrow,
                    "ArrowRight" => Key::RightArrow,
                    "End" => Key::End,
                    "ArrowDown" => Key::DownArrow,
                    "PageDown" => Key::PageDown,
                    #[cfg(not(target_os = "macos"))]
                    "Insert" => Key::Insert,
                    "Delete" => Key::Delete,
                    "VolumeMute" => Key::VolumeMute, // VolumeMute on Firefox, AudioVolumeMute on Chromium
                    "VolumeDown" => Key::VolumeDown, // VolumeDown on Firefox, AudioVolumeDown on Chromium
                    "VolumeUp" => Key::VolumeUp, // VolumeUp on Firefox, AudioVolumeUp on Chromium
                    "NumpadEqual" => Key::Unicode('='),
                    #[cfg(not(target_os = "macos"))]
                    "Pause" => Key::Pause,
                    "NumpadComma" => Key::Unicode(','),
                    "MetaLeft" => Key::Meta, // MetaLeft on Firefox and Chromium
                    "MetaRight" => Key::Meta, // MetaRight on Firefox and Chromium
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    "Undo" => Key::Undo,
                    #[cfg(not(target_os = "macos"))]
                    "Select" => Key::Select,
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    "Find" => Key::Find,
                    "Help" => Key::Help,
                    "MediaTrackNext" => Key::MediaNextTrack,
                    "MediaPlayPause" => Key::MediaPlayPause,
                    "MediaTrackPrevious" => Key::MediaPrevTrack,
                    #[cfg(not(target_os = "macos"))]
                    "MediaStop" => Key::MediaStop,
                    "F13" => Key::F13,
                    "F14" => Key::F14,
                    "F15" => Key::F15,
                    "F16" => Key::F16,
                    "F17" => Key::F17,
                    "F18" => Key::F18,
                    "F19" => Key::F19,
                    "F20" => Key::F20,
                    #[cfg(not(target_os = "macos"))]
                    "F21" => Key::F21,
                    #[cfg(not(target_os = "macos"))]
                    "F22" => Key::F22,
                    #[cfg(not(target_os = "macos"))]
                    "F23" => Key::F23,
                    #[cfg(not(target_os = "macos"))]
                    "F24" => Key::F24,
                    _ => {
                        // Handle any unrecognized keys here
                        println!("Unrecognized key: {}", key);
                        continue;
                    }
                };
                // fix capslock on iPad client
                if key == Key::CapsLock && last_capslock.elapsed() > Duration::from_millis(250) {
                    enigo.key(key, Click).unwrap();
                    last_capslock = Instant::now();
                    continue;
                }
                enigo
                    .key(
                        key,
                        match r#type.as_str() {
                            "keydown" => Press,
                            "keyup" => Release,
                            _ => continue,
                        },
                    )
                    .unwrap();
                // A bug in Safari means that that any keys that are pressed while Meta is held are never released.
                // We work around this by specifically ensuring that all numbers, etc. are released when Meta is released.
                if key == Key::Meta && r#type == "keyup" {
                    let (held, _) = enigo.held();
                    for key in held {
                        enigo.key(key, Release).unwrap();
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}
