use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use enigo::{
    Button, Coordinate,
    Direction::{Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};
use serde::{Deserialize, Serialize};
use streaming::InputCommand;
use tokio::{
    spawn,
    sync::mpsc::{Sender, UnboundedSender},
};

mod streaming;

const PASSWORD: &str = "placeholder";

#[derive(Deserialize)]
struct CreateOffer {
    password: String,
    offer: String,
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
    if payload.password != PASSWORD {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(ResponseOffer::Error("Password incorrect.".to_string())),
        ));
    }
    println!("Killing last session");
    // kill last session
    {
        let mut sender = state.kill_switch.lock().unwrap();
        if let Some(sender) = sender.as_mut()  {
            sender.try_send(()).ok();
        };
        *sender = None;
    }
    println!("Spawning!");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);
    let task = tokio::spawn(streaming::start_video_streaming(
        payload.offer,
        tx,
        state,
    ));
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

async fn home() -> Html<&'static str> {
    Html(include_str!("home.html"))
}

#[derive(Debug, Clone)]
pub struct AppState {
    input_tx: UnboundedSender<InputCommand>,
    kill_switch: Arc<Mutex<Option<Sender<()>>>>,
    width: u32,
    height: u32,
    bitrate: u32,
    startx: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let port = args[1].parse::<u32>().expect("port should be passed as a numerical argument");
    let width = args[2].parse::<u32>().expect("width should be passed as a numerical argument");
    let height = args[3].parse::<u32>().expect("height should be passed as a numerical argument");
    let bitrate = args.get(4).unwrap_or(&"4000".to_owned()).parse::<u32>().expect("bitrate should be passed as a numerical argument");
    let startx = args.get(5).unwrap_or(&"0".to_owned()).parse::<u32>().expect("startx should be passed as a numerical argument");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<InputCommand>();
    let app = Router::new()
        .route("/", get(home))
        .route("/offer", post(offer))
        .layer(tower_http::cors::CorsLayer::very_permissive())
        .with_state(AppState {
            input_tx: tx,
            kill_switch: Arc::new(Mutex::new(None)),
            width,
            height,
            bitrate,
            startx
        });
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    spawn(async { axum::serve(listener, app).await });

    let mut enigo = Enigo::new(&Settings {
        linux_delay: 1,
        ..Default::default()
    })
    .unwrap();

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
                    .move_mouse(x as i32, y as i32, Coordinate::Abs)
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
