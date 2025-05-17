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
use tokio::process::Command;
use tokio::sync::mpsc::unbounded_channel;

use gstreamer::prelude::*;
use gstreamer::{element_error, Element, ElementFactory, Pipeline, State};

use tokio::sync::mpsc::UnboundedReceiver;

use anyhow::{Context, Result};

use log::*;

use crate::Config;

#[cfg(target_os = "linux")]
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

#[derive(Debug)]
pub struct AudioRecordingPipeline {
    pipeline: Pipeline,
    buffer_rx: UnboundedReceiver<(Vec<u8>, u64)>,
}

impl AudioRecordingPipeline {
    #[cfg(not(target_os = "linux"))]
    pub async fn new() -> Result<Self> {
        todo!()
    }

    #[cfg(target_os = "linux")]
    pub async fn new() -> Result<Self> {
        let (buffer_tx, buffer_rx) = unbounded_channel();
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

        let opusenc = ElementFactory::make("opusenc")
            .property("perfect-timestamp", true)
            .build()?;

        let opus_caps = gstreamer::Caps::builder("audio/x-opus").build();

        let appsink = gstreamer_app::AppSink::builder()
            .caps(&opus_caps)
            .drop(true)
            .max_buffers(1)
            .build();

        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
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
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );


        let pipeline = Pipeline::default();
        pipeline.add_many([&src, &src_capsfilter, &opusenc, appsink.upcast_ref()])?;
        Element::link_many([&src, &src_capsfilter, &opusenc, appsink.upcast_ref()])?;

        Ok(Self {
            pipeline,
            buffer_rx,
        })
    }

    pub async fn recv_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        self.buffer_rx.recv().await
    }

    pub fn start_pipeline(&self) {
        let pipeline_clone = self.pipeline.clone();
        tokio::task::spawn_blocking(move || pipeline_clone.set_state(State::Playing).ok());
    }
}

impl Drop for AudioRecordingPipeline {
    fn drop(&mut self) {
        self.pipeline.set_state(State::Null).ok();
    }
}

#[derive(Debug)]
pub struct ScreenRecordingPipeline {
    enc: Element,
    pipeline: Pipeline,
    buffer_rx: UnboundedReceiver<(Vec<u8>, u64)>,
    config: Config,
}

impl ScreenRecordingPipeline {
    #[cfg(target_os = "linux")]
    pub fn new(config: Config, show_mouse: bool) -> Result<Self> {
        let (buffer_tx, buffer_rx) = unbounded_channel();
        let mut elements = vec![];
        let pipeline = Pipeline::default();
        elements.push(
            ElementFactory::make("ximagesrc")
                .property("use-damage", false)
                .property("startx", config.startx)
                .property("starty", config.starty)
                .property_if_some("endx", config.endx)
                .property_if_some("endy", config.endy)
                .property("show-pointer", show_mouse)
                .property("blocksize", 16384u32)
                .property("remote", true)
                .build()?,
        );
        let video_caps = gstreamer::Caps::builder("video/x-raw")
            .field("framerate", gstreamer::Fraction::new(60, 1))
            .build();
        let video_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &video_caps)
            .build()?;
        elements.push(video_capsfilter);

        let videoconvert = if config.vapostproc {
            ElementFactory::make("vapostproc")
                .property_from_str("scale-method", "fast")
                .build()?
        } else {
            ElementFactory::make("videoconvert")
                .property("n-threads", 4u32)
                .build()?
        };

        elements.push(videoconvert);

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
        elements.push(format_capsfilter);

        let enc = if !config.vaapi {
            ElementFactory::make("x264enc")
                .property("threads", 4u32)
                .property("b-adapt", false)
                .property("vbv-buf-capacity", config.vbv_buf_capacity)
                .property_from_str("speed-preset", "superfast")
                .property_from_str("tune", "zerolatency")
                .property("bitrate", config.target_bitrate - 64)
                .property("key-int-max", 2560u32)
                .build()?
        } else {
            ElementFactory::make("vah264enc")
                .property("aud", true)
                .property("b-frames", 0u32)
                .property("dct8x8", false)
                .property("key-int-max", 1024u32)
                .property("num-slices", 4u32)
                .property("ref-frames", 1u32)
                .property("target-usage", 6u32)
                .property_from_str("rate-control", "cbr")
                .property("bitrate", config.target_bitrate - 64)
                .property(
                    "cpb-size",
                    ((config.target_bitrate - 64) * config.vbv_buf_capacity) / 1000,
                )
                .property_from_str("mbbrc", "enabled")
                .build()?
        };

        elements.push(enc.clone());

        let profile = match (config.vaapi, config.full_chroma) {
            (true, _) => "high",
            (_, true) => "high-4:4:4",
            (false, false) => "baseline",
        };

        let final_caps = gstreamer::Caps::builder("video/x-h264")
            .field("profile", profile)
            .field("stream-format", "byte-stream")
            .build();

        let h264_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &final_caps)
            .build()?;

        elements.push(h264_capsfilter);

        let appsink = gstreamer_app::AppSink::builder()
            .caps(&final_caps)
            .drop(true)
            .sync(false)
            .max_buffers(1)
            .build();

        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
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
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        elements.push(appsink.upcast_ref::<Element>().clone());

        info!("Prepared elements: {:?}", &elements);
        pipeline.add_many(&elements)?;
        Element::link_many(&elements)?;

        Ok(Self {
            config,
            enc,
            pipeline,
            buffer_rx,
        })
    }

    #[cfg(target_os = "macos")]
    pub fn new(config: Config, show_mouse: bool) -> Result<Self> {
        let (buffer_tx, buffer_rx) = unbounded_channel();
        let mut elements = vec![];
        let pipeline = Pipeline::default();
        elements.push(
            ElementFactory::make("avfvideosrc")
                .property("capture-screen", true)
                .property("capture-screen-cursor", show_mouse)
                .build()?,
        );
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
        let video_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &video_caps)
            .build()?;
        elements.push(video_capsfilter);

        if config.fullchroma {
            let videoconvert = ElementFactory::make("videoconvert")
                .property("n-threads", 4u32)
                .build()?;

            let format_capsfilter = ElementFactory::make("capsfilter")
                .property(
                    "caps",
                    gstreamer::Caps::builder("video/x-raw")
                        .field("format", "Y444")
                        .build(),
                )
                .build()?;
            elements.push(videoconvert);
            elements.push(format_capsfilter);
        }

        let enc = if !config.vaapi {
            ElementFactory::make("x264enc")
                .property("threads", 4u32)
                .property("b-adapt", false)
                .property("vbv-buf-capacity", config.vbv_buf_capacity)
                .property_from_str("speed-preset", "superfast")
                .property_from_str("tune", "zerolatency")
                .property("bitrate", config.target_bitrate - 64)
                .property("key-int-max", 2560u32)
                .build()?
        } else {
            ElementFactory::make("vtenc_h264")
                .property("allow-frame-reordering", false)
                .property("bitrate", config.target_bitrate - 64)
                .property("realtime", true)
                .build()?
        };

        elements.push(enc.clone());

        let final_caps = if config.vaapi {
            gstreamer::Caps::builder("video/x-h264")
                .field("stream-format", "avc")
                .build()
        } else {
            let profile = if config.full_chroma {
                "high-4:4:4"
            } else {
                "baseline"
            };

            gstreamer::Caps::builder("video/x-h264")
                .field("profile", profile)
                .field("stream-format", "byte-stream")
                .build()
        };

        let h264_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &final_caps)
            .build()?;

        elements.push(h264_capsfilter);

        let parse = ElementFactory::make("h264parse")
            .property("config-interval", -1)
            .build()?;
        let final_caps = gstreamer::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .build();
        let parse_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &final_caps)
            .build()?;

        elements.push(parse_capsfilter);

        let appsink = gstreamer_app::AppSink::builder()
            .caps(&final_caps)
            .drop(true)
            .sync(false)
            .max_buffers(1)
            .build();

        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
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
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        elements.push(appsink.upcast_ref::<Element>().clone());

        pipeline.add_many(&elements)?;
        Element::link_many(&elements)?;

        Ok(Self {
            config,
            enc,
            pipeline,
            buffer_rx,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn new(config: Config, show_mouse: bool) -> Result<Self> {
        let (buffer_tx, buffer_rx) = unbounded_channel();
        let mut elements = vec![];
        let pipeline = Pipeline::default();
        let src = ElementFactory::make("d3d11screencapturesrc")
            .property("show-cursor", show_mouse)
            .build()?;
        elements.push(src);

        let video_caps = if !config.vaapi {
            gstreamer::Caps::builder("video/x-raw")
                .field("framerate", gstreamer::Fraction::new(60, 1))
                .build()
        } else {
            let caps_str = "video/x-raw(memory:D3D11Memory),framerate=60/1";
            gstreamer::Caps::from_str(caps_str)?
        };

        let video_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &video_caps)
            .build()?;
        elements.push(video_capsfilter);

        let videoconvert = if !config.vaapi {
            ElementFactory::make("videoconvert")
                .property("n-threads", 4u32)
                .build()?
        } else {
            ElementFactory::make("d3d11convert")
                .build()?
        };

        elements.push(videoconvert);

        let format = if config.full_chroma { "Y444" } else { "NV12" };

        let format_caps = if !config.vaapi {
            gstreamer::Caps::builder("video/x-raw")
                .field("format", format)
                .build()
        } else {
            let caps_str = "video/x-raw(memory:D3D11Memory),format=NV12";
            gstreamer::Caps::from_str(caps_str)?
        };

        info!("Format caps: {}", format_caps);

        let format_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &format_caps)
            .build()?;
        elements.push(format_capsfilter);

        let enc = if !config.vaapi {
            ElementFactory::make("x264enc")
                .property("threads", 4u32)
                .property("b-adapt", false)
                .property("vbv-buf-capacity", config.vbv_buf_capacity)
                .property_from_str("speed-preset", "superfast")
                .property_from_str("tune", "zerolatency")
                .property("bitrate", config.target_bitrate - 64)
                .property("key-int-max", 2560u32)
                .build()?
        } else {
            ElementFactory::make("mfh264enc")
                .property("low-latency", true)
                .property("bframes", 0u32)
                .property("cabac", false)
                .property_from_str("rc-mode", "cbr")
                .property("bitrate", config.target_bitrate - 64)
                .property("vbv-buffer-size", config.vbv_buf_capacity)
                .property("gop-size", 2560i32)
                .property("quality-vs-speed", 100u32)
                .build()?
        };

        elements.push(enc.clone());

        let profile = match (config.vaapi, config.full_chroma) {
            (true, _) => "high",
            (_, true) => "high-4:4:4",
            (false, false) => "baseline",
        };

        let final_caps = gstreamer::Caps::builder("video/x-h264")
            .field("profile", profile)
            .field("stream-format", "byte-stream")
            .build();

        let h264_capsfilter = ElementFactory::make("capsfilter")
            .property("caps", &final_caps)
            .build()?;

        elements.push(h264_capsfilter);

        let appsink = gstreamer_app::AppSink::builder()
            .caps(&final_caps)
            .drop(true)
            .sync(false)
            .max_buffers(1)
            .build();

        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
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
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        elements.push(appsink.upcast_ref::<Element>().clone());

        info!("Prepared elements: {:?}", &elements);
        pipeline.add_many(&elements)?;
        Element::link_many(&elements)?;

        Ok(Self {
            config,
            enc,
            pipeline,
            buffer_rx,
        })
    }

    pub fn set_bitrate(&self, new_bitrate: u32) {
        if self.config.vaapi {
            // Setting bitrate on macOS causes it vtenc_h264 to DEADLOCK
            // YET ANOTHER ASTOUNDINGLY BROKEN PIECE OF SOFTWARE
            // WRITTEN BY THE """DEVELOPERS""" AT APPLE INC
            // MANY SUCH CASES!
            #[cfg(not(target_os = "macos"))]
            self.enc.set_property("bitrate", new_bitrate);
            #[cfg(target_os = "linux")]
            self.enc.set_property(
                "cpb-size",
                (new_bitrate * self.config.vbv_buf_capacity) / 1000,
            );
        } else {
            self.enc.set_property("bitrate", new_bitrate);
        }
    }

    pub fn force_keyframe(&self) {
        info!("Forcing keyframe");

        if !(cfg!(target_os = "macos") && self.config.vaapi) {
            let force_keyframe_event = gstreamer::Structure::builder("GstForceKeyUnit").build();

            // Send the event to the encoder element
            self.enc.send_event(gstreamer::event::CustomDownstream::new(
                force_keyframe_event,
            ));
        }
    }

    pub async fn recv_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        self.buffer_rx.recv().await
    }

    pub fn start_pipeline(&self) {
        let pipeline_clone = self.pipeline.clone();
        tokio::task::spawn_blocking(move || pipeline_clone.set_state(State::Playing).ok());
    }
}

impl Drop for ScreenRecordingPipeline {
    fn drop(&mut self) {
        self.pipeline.set_state(State::Null).ok();
    }
}
