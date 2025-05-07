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

use std::collections::HashSet;
use std::time::{Duration, Instant};

use log::*;

use input_device::{InputSimulator, Key};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedReceiver;

use strum::IntoEnumIterator;

pub fn browser_code_to_key(code: &str) -> Option<Key> {
    match code {
        // --- Top Row (Function Keys) ---
        "Escape" => Some(Key::Esc),
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        "F13" => Some(Key::F13),
        "F14" => Some(Key::F14),
        "F15" => Some(Key::F15),
        // F16-F22 omitted as they are less common and not in the initial enum
        "F23" => Some(Key::F23),
        // F24+ omitted

        // --- Top Row (Number Row) ---
        "Backquote" => Some(Key::Grave), // Often `~ key
        "Digit1" => Some(Key::Num1),
        "Digit2" => Some(Key::Num2),
        "Digit3" => Some(Key::Num3),
        "Digit4" => Some(Key::Num4),
        "Digit5" => Some(Key::Num5),
        "Digit6" => Some(Key::Num6),
        "Digit7" => Some(Key::Num7),
        "Digit8" => Some(Key::Num8),
        "Digit9" => Some(Key::Num9),
        "Digit0" => Some(Key::Num0),
        "Minus" => Some(Key::Minus),
        "Equal" => Some(Key::Equal),
        "Backspace" => Some(Key::Backspace),

        // --- Second Row (QWERTY Row) ---
        "Tab" => Some(Key::Tab),
        "KeyQ" => Some(Key::Q),
        "KeyW" => Some(Key::W),
        "KeyE" => Some(Key::E),
        "KeyR" => Some(Key::R),
        "KeyT" => Some(Key::T),
        "KeyY" => Some(Key::Y),
        "KeyU" => Some(Key::U),
        "KeyI" => Some(Key::I),
        "KeyO" => Some(Key::O),
        "KeyP" => Some(Key::P),
        "BracketLeft" => Some(Key::LeftBrace), // Often [{ key
        "BracketRight" => Some(Key::RightBrace), // Often ]} key
        "Enter" => Some(Key::Enter),           // Main Enter key

        // --- Third Row (Home Row) ---
        "CapsLock" => Some(Key::CapsLock),
        "KeyA" => Some(Key::A),
        "KeyS" => Some(Key::S),
        "KeyD" => Some(Key::D),
        "KeyF" => Some(Key::F),
        "KeyG" => Some(Key::G),
        "KeyH" => Some(Key::H),
        "KeyJ" => Some(Key::J),
        "KeyK" => Some(Key::K),
        "KeyL" => Some(Key::L),
        "Semicolon" => Some(Key::Semicolon), // Often ;: key
        "Quote" => Some(Key::Apostrophe),    // Often '" key
        "Backslash" => Some(Key::Backslash), // Often \| key (ANSI)

        // --- Fourth Row (Bottom Row) ---
        "ShiftLeft" => Some(Key::LeftShift),
        "IntlBackslash" => Some(Key::IntlBackslash), // ISO key often between LShift and Z
        "KeyZ" => Some(Key::Z),
        "KeyX" => Some(Key::X),
        "KeyC" => Some(Key::C),
        "KeyV" => Some(Key::V),
        "KeyB" => Some(Key::B),
        "KeyN" => Some(Key::N),
        "KeyM" => Some(Key::M),
        "Comma" => Some(Key::Comma), // Often ,< key
        "Period" => Some(Key::Dot),  // Often .> key
        "Slash" => Some(Key::Slash), // Often /? key
        "ShiftRight" => Some(Key::RightShift),

        // --- Bottom Row (Control Row) ---
        "ControlLeft" => Some(Key::LeftCtrl),
        "MetaLeft" => Some(Key::LeftMeta), // Windows/Super/Command key
        "AltLeft" => Some(Key::LeftAlt),
        "Space" => Some(Key::Space),
        "AltRight" => Some(Key::RightAlt),   // Often AltGr
        "MetaRight" => Some(Key::RightMeta), // Windows/Super/Command key
        "ContextMenu" => Some(Key::Compose), // Menu key
        "ControlRight" => Some(Key::RightCtrl),

        // --- Navigation Block ---
        "PrintScreen" => Some(Key::SysRq), // Often SysRq/PrintScreen key
        "ScrollLock" => Some(Key::ScrollLock),
        "Pause" => Some(Key::Pause), // Often Pause/Break key

        "Insert" => Some(Key::Insert),
        "Home" => Some(Key::Home),
        "PageUp" => Some(Key::PageUp),
        "Delete" => Some(Key::Delete),
        "End" => Some(Key::End),
        "PageDown" => Some(Key::PageDown),

        // --- Arrow Keys ---
        "ArrowUp" => Some(Key::Up),
        "ArrowLeft" => Some(Key::Left),
        "ArrowDown" => Some(Key::Down),
        "ArrowRight" => Some(Key::Right),

        // --- Numpad ---
        "NumLock" => Some(Key::NumLock),
        "NumpadDivide" => Some(Key::KpSlash),
        "NumpadMultiply" => Some(Key::KpAsterisk),
        "NumpadSubtract" => Some(Key::KpMinus),
        "NumpadAdd" => Some(Key::KpPlus),
        "NumpadEnter" => Some(Key::KpEnter),
        "NumpadDecimal" => Some(Key::KpDot),
        "NumpadComma" => Some(Key::KpComma), // Some numpads have comma instead/as well
        "NumpadEqual" => Some(Key::KpEqual), // Less common
        // Numpad Paren, +/- etc omitted as they often require Shift or aren't standard codes
        "Numpad0" => Some(Key::Kp0),
        "Numpad1" => Some(Key::Kp1),
        "Numpad2" => Some(Key::Kp2),
        "Numpad3" => Some(Key::Kp3),
        "Numpad4" => Some(Key::Kp4),
        "Numpad5" => Some(Key::Kp5),
        "Numpad6" => Some(Key::Kp6),
        "Numpad7" => Some(Key::Kp7),
        "Numpad8" => Some(Key::Kp8),
        "Numpad9" => Some(Key::Kp9),

        // --- Japanese Keyboard Specific ---
        "IntlRo" => Some(Key::Ro), // Usually '\ろ' key
        "Katakana" => Some(Key::Katakana),
        "Hiragana" => Some(Key::Hiragana), // Often shared with Katakana key
        "KatakanaHiragana" => Some(Key::KatakanaHiragana), // Often the same key as above
        "ZenkakuHankaku" => Some(Key::ZenkakuHankaku), // Often `~ key on JIS layout
        "Henkan" => Some(Key::Henkan),     // Convert key
        "Muhenkan" => Some(Key::Muhenkan), // Non-convert key
        "IntlYen" => Some(Key::Yen),       // Usually '¥|' key

        // --- Korean Keyboard Specific ---
        "Lang1" => Some(Key::Hanguel), // Often Hangul/English toggle
        "Lang2" => Some(Key::Hanja),   // Often Hanja key

        // --- Multimedia Keys (Common Mappings) ---
        "AudioVolumeMute" | "VolumeMute" => Some(Key::Mute),
        "AudioVolumeDown" | "VolumeDown" => Some(Key::VolumeDown),
        "AudioVolumeUp" | "VolumeUp" => Some(Key::VolumeUp),
        "MediaTrackNext" => Some(Key::NextSong),
        "MediaTrackPrevious" => Some(Key::PreviousSong),
        "MediaStop" => Some(Key::StopCD), // Or Key::Stop if more general
        "MediaPlayPause" => Some(Key::PlayPause),
        "LaunchMail" => Some(Key::Mail),
        "LaunchApp2" | "SelectMedia" => Some(Key::Media), // Often Launch Media Player
        "LaunchApp1" => Some(Key::Calc),                  // Often Launch Calculator
        "BrowserSearch" => Some(Key::Search),
        "BrowserHome" => Some(Key::Homepage),
        "BrowserBack" => Some(Key::Back),
        "BrowserForward" => Some(Key::Forward),
        "BrowserStop" => Some(Key::Stop), // General stop, different from MediaStop?
        "BrowserRefresh" => Some(Key::Refresh),
        "BrowserFavorites" => Some(Key::Bookmarks),

        // --- Power/Sleep ---
        "Power" => Some(Key::Power),
        "Sleep" => Some(Key::Sleep),
        "WakeUp" => Some(Key::WakeUp),

        // --- Less Common / Laptop keys ---
        // These might vary significantly or not report standard codes
        "BrightnessDown" => Some(Key::BrightnessDown),
        "BrightnessUp" => Some(Key::BrightnessUp),
        "Eject" => None,           // Not in Key enum, could map if needed
        "Help" => Some(Key::Help), // Sometimes mapped to Insert

        // --- Unidentified or Unmappable ---
        "Unidentified" => None, // Explicitly ignore
        _ => None,              // Any code not listed above is not mapped
    }
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InputCommand {
    pub r#type: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub button: Option<u8>,
    pub key: Option<String>,
    pub id: Option<i32>,
    pub pressure: Option<f64>,
    pub tiltX: Option<i32>,
    pub tiltY: Option<i32>,
}

pub fn do_input(
    mut rx: UnboundedReceiver<InputCommand>,
    startx: u32,
    starty: u32,
) -> anyhow::Result<()> {
    let mut sim = InputSimulator::new()?;

    let mut last_capslock = Instant::now();

    let mut held: HashSet<Key> = HashSet::new();

    while let Some(msg) = rx.blocking_recv() {
        match msg {
            InputCommand {
                r#type,
                x: Some(x),
                y: Some(y),
                pressure: Some(pressure),
                tiltX: Some(tilt_x),
                tiltY: Some(tilt_y),
                ..
            } => {
                if r#type == "pen" {
                    sim.pen(
                        x + startx as i32,
                        y + starty as i32,
                        pressure,
                        tilt_x,
                        tilt_y,
                    )
                    .ok();
                }
            }
            InputCommand {
                r#type,
                x: Some(x),
                y: Some(y),
                id: Some(id),
                ..
            } => match r#type.as_str() {
                "touchstart" => {
                    sim.touch_down(id, x + startx as i32, y + starty as i32)
                        .ok();
                }
                "touchmove" => {
                    sim.touch_move(id, x + startx as i32, y + starty as i32)
                        .ok();
                }
                _ => {}
            },
            InputCommand {
                r#type,
                x: Some(x),
                y: Some(y),
                ..
            } => match r#type.as_str() {
                "mousemove" => {
                    sim.move_mouse_rel(x, y)?;
                }
                "mousemoveabs" => sim.move_mouse_abs(x + startx as i32, y + starty as i32)?,
                "wheel" => {
                    sim.wheel(x, -y)?;
                }
                _ => {}
            },
            InputCommand {
                r#type,
                id: Some(id),
                ..
            } => {
                if r#type.as_str() == "touchend" {
                    sim.touch_up(id)?;
                }
            }
            InputCommand {
                r#type,
                button: Some(button),
                ..
            } => match (button, r#type.as_str()) {
                (0, "mousedown") => sim.left_mouse_down()?,
                (0, "mouseup") => sim.left_mouse_up()?,
                (1, "mousedown") => sim.middle_mouse_down()?,
                (1, "mouseup") => sim.middle_mouse_up()?,
                (2, "mousedown") => sim.right_mouse_down()?,
                (2, "mouseup") => sim.right_mouse_up()?,
                _ => error!("Received bad mouse button: {}", button),
            },
            InputCommand {
                r#type,
                key: Some(key),
                ..
            } => {
                let parsed_key = browser_code_to_key(&key);
                if let Some(key) = parsed_key {
                    // fix capslock on iPad client
                    if key == Key::CapsLock && last_capslock.elapsed() > Duration::from_millis(250)
                    {
                        sim.key_down(key)?;
                        //std::thread::sleep(Duration::from_millis(16));
                        sim.key_up(key)?;
                        last_capslock = Instant::now();
                        continue;
                    }
                    match r#type.as_str() {
                        "keydown" => {
                            sim.key_down(key)?;
                            held.insert(key);
                        }
                        "keyup" => sim.key_up(key)?,
                        _ => error!("Received bad packet type: {}", r#type),
                    }

                    // A SEVERE BUG in Safari means that that any keys that are pressed while Meta is held are never released.
                    // We work around this by specifically ensuring that all numbers, etc. are released when Meta is released.
                    if (key == Key::LeftMeta || key == Key::RightMeta) && r#type == "keyup" {
                        for key in held.iter() {
                            sim.key_up(*key)?;
                        }
                        held.clear();
                    }
                } else {
                    error!("Received unknown key: {}", key);
                }
            }
            InputCommand { r#type, .. } => {
                if r#type == "resetkeyboard" {
                    let keys = Key::iter();
                    // Unpress all possible keys
                    for key in keys {
                        sim.key_up(key)?;
                    }
                }
            }
        }
    }

    Ok(())
}
