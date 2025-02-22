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

#[allow(unused)]
use std::str::{self, FromStr};
use std::sync::Arc;
use tokio::process::Command;

use gstreamer::prelude::*;
use gstreamer::{element_error, ElementFactory, Pipeline, State};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;

use anyhow::{Context, Result};

use log::*;

use crate::Config;

use super::GStreamerControlMessage;

async fn get_pulseaudio_monitor_name() -> Result<String> {
    let output = Command::new("pactl")
        .arg("list")
        .arg("sources")
        .output()
        .await
        .context("Failed to execute pactl")?;

    if !output.status.success() {
        anyhow::bail!("pactl exited with status: {}", output.status);
    }

    let stdout = str::from_utf8(&output.stdout).context("Invalid UTF-8 output")?;

    for line in stdout.lines() {
        if line.trim_start().starts_with("Name:") {
            let name = line
                .split_whitespace()
                .nth(1)
                .context("Could not parse Name line")?;

            if name.contains("monitor") {
                return Ok(name.to_string());
            }
        }
    }

    anyhow::bail!("No monitor device found");
}

/// # Warning: LINUX ONLY
pub async fn start_audio_pipeline(
    mut control_rx: UnboundedReceiver<GStreamerControlMessage>,
    buffer_tx: UnboundedSender<(Vec<u8>, u64)>,
    waker: Arc<Notify>,
) -> anyhow::Result<()> {
    // gst-launch-1.0 -v pulsesrc device=alsa_output.pci-0000_00_1f.3.analog-stereo.monitor ! audioconvert ! vorbisenc ! oggmux ! filesink location=alsasrc.ogg
    let monitor_device_name = get_pulseaudio_monitor_name().await?;
    info!("Picked audio monitor device name: {}", monitor_device_name);
    let src = ElementFactory::make("pulsesrc")
        .property("device", &monitor_device_name)
        .build()?;
    let src_capsfilter = ElementFactory::make("capsfilter")
        .property(
            "caps",
            gstreamer::Caps::builder("audio/x-raw")
                .field("channels", 2)
                .build(),
        )
        .build()?;

    let opusenc = ElementFactory::make("opusenc").build()?;

    let opus_caps = gstreamer::Caps::builder("audio/x-opus").build();

    let appsink = gstreamer_app::AppSink::builder()
        // Tell the appsink what format we want. It will then be the audiotestsrc's job to
        // provide the format we request.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        .caps(&opus_caps)
        //.drop(true)
        .build();

    // appsink callback - send rtp packets to the streaming thread
    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            // Add a handler to the "new-sample" signal.
            .new_sample(move |appsink| {
                // Pull the sample in question out of the appsink's buffer.
                let sample = appsink
                    .pull_sample()
                    .map_err(|_| gstreamer::FlowError::Eos)?;
                let buffer = sample.buffer().ok_or_else(|| {
                    element_error!(
                        appsink,
                        gstreamer::ResourceError::Failed,
                        ("Failed to get buffer from appsink")
                    );

                    gstreamer::FlowError::Error
                })?;

                // At this point, buffer is only a reference to an existing memory region somewhere.
                // When we want to access its content, we have to map it while requesting the required
                // mode of access (read, read/write).
                // This type of abstraction is necessary, because the buffer in question might not be
                // on the machine's main memory itself, but rather in the GPU's memory.
                // So mapping the buffer makes the underlying memory region accessible to us.
                // See: https://gstreamer.freedesktop.org/documentation/plugin-development/advanced/allocation.html
                let map = buffer.map_readable().map_err(|_| {
                    element_error!(
                        appsink,
                        gstreamer::ResourceError::Failed,
                        ("Failed to map buffer readable")
                    );

                    gstreamer::FlowError::Error
                })?;

                let packet = map.as_slice();

                let pts = buffer.pts().unwrap().useconds();

                // we can .ok() this, because if it DOES fail, the thread will be terminated soon
                buffer_tx.send((packet.to_vec(), pts)).ok();
                waker.notify_one();
                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

    // Create the pipeline
    let pipeline = Pipeline::default();

    // Add elements to the pipeline
    pipeline.add_many([&src, &src_capsfilter, &opusenc, appsink.upcast_ref()])?;

    // Link the elements
    gstreamer::Element::link_many([&src, &src_capsfilter, &opusenc, appsink.upcast_ref()])?;

    // Set the pipeline to playing state
    pipeline.set_state(State::Playing)?;
    while let Some(msg) = control_rx.recv().await {
        if let GStreamerControlMessage::Stop = msg {
            info!("GStreamer task received termination signal!");
            break;
        }
    }

    // Clean up
    pipeline.set_state(State::Null)?;

    Ok(())
}

pub async fn start_pipeline(
    config: Config,
    show_mouse: bool,
    mut control_rx: UnboundedReceiver<GStreamerControlMessage>,
    buffer_tx: UnboundedSender<(Vec<u8>, u64)>,
    waker: Arc<Notify>,
) -> anyhow::Result<()> {
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let src = ElementFactory::make("ximagesrc")
        .property("use-damage", false)
        .property("startx", config.startx)
        .property("show-pointer", show_mouse)
        .property("blocksize", 16384u32)
        .property("remote", true)
        .build()?;

    #[cfg(target_os = "macos")]
    let src = ElementFactory::make("avfvideosrc")
        .property("capture-screen", true)
        .property("capture-screen-cursor", show_mouse)
        .build()?;

    #[cfg(target_os = "windows")]
    let src = ElementFactory::make("d3d11screencapturesrc")
        .property("show-cursor", show_mouse)
        .build()?;

    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            let video_caps = if !config.full_chroma {
                gstreamer::Caps::builder("video/x-raw")
                    .field("framerate", gstreamer::Fraction::new(60, 1))
                    .field("format", "NV12")
                    .build()
            } else {
                gstreamer::Caps::builder("video/x-raw")
                    .field("framerate", gstreamer::Fraction::new(60, 1))
                    .field("format", "BGRA")
                    .build()
            };
        } else {
            let video_caps = gstreamer::Caps::builder("video/x-raw")
                .field("framerate", gstreamer::Fraction::new(60, 1))
                .build();
        }
    }

    let video_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &video_caps)
        .build()?;

    cfg_if::cfg_if! {
        if #[cfg(not(target_os = "macos"))] {
            let videoconvert = if config.vapostproc {
                ElementFactory::make("vapostproc")
                    .property_from_str("scale-method", "fast")
                    .build()?
            } else {
                ElementFactory::make("videoconvert")
                    .property("n-threads", 4u32)
                    .build()?
            };

            let format = if config.full_chroma { "Y444" } else { "NV12" };

            if config.full_chroma && config.vapostproc {
                warn!(
                    "Full-chroma is not supported with VA-API! This configuration option has been ignored."
                );
            }

            let format_caps = if !config.vapostproc {
                gstreamer::Caps::builder("video/x-raw")
                    .field("format", format)
                    .build()
            } else {
                let caps_str = "video/x-raw(memory:VAMemory)";
                gstreamer::Caps::from_str(caps_str)?
            };

            info!("Format caps: {}", format_caps);
            let format_capsfilter = ElementFactory::make("capsfilter")
                .property("caps", &format_caps)
                .build()?;
        }
    }

    // this makes the stream smoother on VAAPI, but on x264enc it causes severe latency
    // especially when the CPU is under load (such as when playing minecraft)
    // #[cfg(feature = "vaapi")]
    // let conversion_queue = ElementFactory::make("queue").build()?;

    let enc = if !config.vaapi {
        ElementFactory::make("x264enc")
            //.property("qos", true)
            .property("threads", 4u32)
            .property("aud", true)
            .property("b-adapt", false)
            .property("bframes", 0u32)
            .property("insert-vui", true)
            .property("rc-lookahead", 0)
            .property("vbv-buf-capacity", config.vbv_buf_capacity)
            .property("sliced-threads", true)
            .property("byte-stream", true)
            .property_from_str("pass", "cbr")
            .property_from_str("speed-preset", "veryfast")
            .property_from_str("tune", "zerolatency")
            .property("bitrate", 4000u32 - 64u32)
            .build()?
    } else {
        cfg_if::cfg_if! {
            if #[cfg(target_os = "linux")] {
                ElementFactory::make("vah264enc")
                    .property("aud", true)
                    .property("b-frames", 0u32)
                    .property("dct8x8", false)
                    .property("key-int-max", 1024u32)
                    .property("num-slices", 4u32)
                    .property("ref-frames", 1u32)
                    .property("target-usage", 6u32)
                    .property_from_str("rate-control", "cbr")
                    .property("bitrate", 4000u32 - 64u32)
                    .property(
                        "cpb-size",
                        ((4000u32 - 64u32) * config.vbv_buf_capacity) / 1000,
                    )
                    .property_from_str("mbbrc", "enabled")
                    .build()?
            } else if #[cfg(target_os = "macos")] {
                ElementFactory::make("vtenc_h264")
                    .property("allow-frame-reordering", false)
                    .property("bitrate", 4000u32 - 64u32)
                    .property("realtime", true)
                    .build()?
            } else {
                bail!("Hardware accelerated encoding is only supported on macOS and Linux.");
            }
        }
    };

    info!("Enc: {:?}", enc);

    let profile = if config.full_chroma {
        "high-4:4:4"
    } else {
        "baseline"
    };

    let final_caps = if config.vaapi {
        cfg_if::cfg_if! {
            if #[cfg(target_os = "macos")] {
                gstreamer::Caps::builder("video/x-h264")
                    .field("stream-format", "avc")
                    .build()
            } else if #[cfg(target_os = "linux")] {
                gstreamer::Caps::builder("video/x-h264")
                    .field("profile", "high")
                    .field("stream-format", "byte-stream")
                    .build()
            } else {
                bail!("Hardware accelerated encoding is only supported on macOS and Linux.");
            }
        }
    } else {
        gstreamer::Caps::builder("video/x-h264")
            .field("profile", profile)
            .field("stream-format", "byte-stream")
            .build()
    };

    let h264_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &final_caps)
        .build()?;

    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            let parse = ElementFactory::make("h264parse").property("config-interval", -1).build()?;
            let final_caps = gstreamer::Caps::builder("video/x-h264")
                .field("stream-format", "byte-stream")
                .build();
            let parse_capsfilter = ElementFactory::make("capsfilter")
                .property("caps", &final_caps)
                .build()?;
        }
    }

    let appsink = gstreamer_app::AppSink::builder()
        // Tell the appsink what format we want. It will then be the audiotestsrc's job to
        // provide the format we request.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        .caps(&final_caps)
        //.drop(true)
        .build();

    // appsink callback - send rtp packets to the streaming thread
    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            // Add a handler to the "new-sample" signal.
            .new_sample(move |appsink| {
                // Pull the sample in question out of the appsink's buffer.
                let sample = appsink
                    .pull_sample()
                    .map_err(|_| gstreamer::FlowError::Eos)?;
                let buffer = sample.buffer().ok_or_else(|| {
                    element_error!(
                        appsink,
                        gstreamer::ResourceError::Failed,
                        ("Failed to get buffer from appsink")
                    );

                    gstreamer::FlowError::Error
                })?;

                // At this point, buffer is only a reference to an existing memory region somewhere.
                // When we want to access its content, we have to map it while requesting the required
                // mode of access (read, read/write).
                // This type of abstraction is necessary, because the buffer in question might not be
                // on the machine's main memory itself, but rather in the GPU's memory.
                // So mapping the buffer makes the underlying memory region accessible to us.
                // See: https://gstreamer.freedesktop.org/documentation/plugin-development/advanced/allocation.html
                let map = buffer.map_readable().map_err(|_| {
                    element_error!(
                        appsink,
                        gstreamer::ResourceError::Failed,
                        ("Failed to map buffer readable")
                    );

                    gstreamer::FlowError::Error
                })?;

                let packet = map.as_slice();

                let pts = buffer.pts().unwrap().useconds();

                // we can .ok() this, because if it DOES fail, the thread will be terminated soon
                buffer_tx.send((packet.to_vec(), pts)).ok();
                waker.notify_one();
                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

    //let queue = ElementFactory::make("queue").property("max-size-buffers", 1u32).property("max-size-time", 0u64).property("max-size-bytes", 0u32).property_from_str("leaky", "downstream").build()?;

    // Create the pipeline
    let pipeline = Pipeline::default();

    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            if config.full_chroma {
                let videoconvert = ElementFactory::make("videoconvert")
                    .property("n-threads", 4u32)
                    .build()?;

                let format_capsfilter = ElementFactory::make("capsfilter")
                    .property(
                        "caps",
                        gstreamer::Caps::builder("video/x-raw")
                            .field("format", "Y444")
                            .build()
                    )
                    .build()?;

                // Add elements to the pipeline
                pipeline.add_many([
                    &src,
                    &video_capsfilter,
                    &videoconvert,
                    &format_capsfilter,
                    &enc,
                    &h264_capsfilter,
                    appsink.upcast_ref(),
                ])?;

                // Link the elements
                gstreamer::Element::link_many([
                    &src,
                    &video_capsfilter,
                    &videoconvert,
                    &format_capsfilter,
                    &enc,
                    &h264_capsfilter,
                    appsink.upcast_ref(),
                ])?;
            } else if !config.vaapi {
                // Add elements to the pipeline
                pipeline.add_many([
                    &src,
                    &video_capsfilter,
                    //&queue,
                    &enc,
                    &h264_capsfilter,
                    appsink.upcast_ref(),
                ])?;

                // Link the elements
                gstreamer::Element::link_many([
                    &src,
                    &video_capsfilter,
                    //&queue,
                    &enc,
                    &h264_capsfilter,
                    appsink.upcast_ref(),
                ])?;
            } else {
                // Add elements to the pipeline
                pipeline.add_many([
                    &src,
                    &video_capsfilter,
                    //&queue,
                    &enc,
                    &h264_capsfilter,
                    &parse,
                    &parse_capsfilter,
                    appsink.upcast_ref(),
                ])?;

                // Link the elements
                gstreamer::Element::link_many([
                    &src,
                    &video_capsfilter,
                    //&queue,
                    &enc,
                    &h264_capsfilter,
                    &parse,
                    &parse_capsfilter,
                    appsink.upcast_ref(),
                ])?;
            }
        } else {
            // Add elements to the pipeline
            pipeline.add_many([
                &src,
                &video_capsfilter,
                &videoconvert,
                &format_capsfilter,
                //&queue,
                &enc,
                &h264_capsfilter,
                appsink.upcast_ref(),
            ])?;

            // Link the elements
            gstreamer::Element::link_many([
                &src,
                &video_capsfilter,
                &videoconvert,
                &format_capsfilter,
                //&queue,
                &enc,
                &h264_capsfilter,
                appsink.upcast_ref(),
            ])?;
        }
    }

    // Set the pipeline to playing state
    pipeline.set_state(State::Playing)?;

    while let Some(msg) = control_rx.recv().await {
        match msg {
            GStreamerControlMessage::Stop => {
                info!("GStreamer task received termination signal!");
                break;
            }
            GStreamerControlMessage::RequestKeyFrame => {
                info!("Forcing keyframe");

                if !(cfg!(target_os = "macos") && config.vaapi) {
                    let force_keyframe_event =
                        gstreamer::Structure::builder("GstForceKeyUnit").build();

                    // Send the event to the encoder element
                    enc.send_event(gstreamer::event::CustomDownstream::new(
                        force_keyframe_event,
                    ));
                }
            }
            GStreamerControlMessage::Bitrate(bitrate) => {
                if config.vaapi {
                    // Setting bitrate on macOS causes it vtenc_h264 to DEADLOCK
                    // YET ANOTHER ASTOUNDINGLY BROKEN PIECE OF SOFTWARE
                    // WRITTEM BY THE """DEVELOPERS""" AT APPLE INC
                    // MANY SUCH CASES!
                    #[cfg(not(target_os = "macos"))]
                    enc.set_property("bitrate", bitrate);
                    #[cfg(not(target_os = "macos"))]
                    enc.set_property(
                        "cpb-size",
                        ((4000u32 - 64u32) * config.vbv_buf_capacity) / 1000,
                    );
                } else {
                    enc.set_property("bitrate", bitrate);
                }
            }
        }
    }

    // Clean up
    pipeline.set_state(State::Null)?;

    Ok(())
}
