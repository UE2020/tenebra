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
use std::str::FromStr;
use std::sync::Arc;

use gstreamer::prelude::*;
use gstreamer::{element_error, ElementFactory, Pipeline, State};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;

use super::GStreamerControlMessage;

#[derive(Default)]
struct StatisticsOverlay {
    bitrate: Option<u32>,
    rtt: Option<f32>,
    loss: Option<f32>,
}

impl StatisticsOverlay {
    const fn new() -> Self {
        StatisticsOverlay {
            bitrate: None,
            rtt: None,
            loss: None,
        }
    }

    fn render_to(&self, textoverlay: &gstreamer::Element) {
        let mut text = String::new();
        if let Some(bitrate) = self.bitrate {
            text.push_str(&format!("Bitrate: {} kbit/s\n", bitrate));
        }
        if let Some(rtt) = self.rtt {
            text.push_str(&format!("Round-trip: {} ms\n", rtt.round()));
        }
        if let Some(loss) = self.loss {
            text.push_str(&format!("Loss: {}%", (loss * 100.0).round()));
        }
        textoverlay.set_property("text", text);
    }
}

pub async fn start_pipeline(
    startx: u32,
    show_mouse: bool,
    mut control_rx: UnboundedReceiver<GStreamerControlMessage>,
    buffer_tx: UnboundedSender<Vec<u8>>,
    waker: Arc<Notify>,
) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    let src = ElementFactory::make("ximagesrc")
        .property("use-damage", false)
        .property("startx", startx)
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
    let src = ElementFactory::make("d3d12screencapturesrc")
        .property("show-cursor", show_mouse)
        .build()?;

    let textoverlay = ElementFactory::make("textoverlay")
        .property("text", "")
        .property_from_str("valignment", "bottom")
        .property_from_str("halignment", "center")
        .property("font-desc", "Sans, 3")
        //.property("draw-outline", false)
        //.property("draw-shadow", false)
        .property("ypad", 3i32)
        //.property("color", u32::from_ne_bytes([0, 0, 255, 255]))
        .build()?;

    let video_caps = gstreamer::Caps::builder("video/x-raw")
        .field("framerate", gstreamer::Fraction::new(60, 1))
        .build();
    let video_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &video_caps)
        .build()?;

    #[cfg(not(feature = "vaapi"))]
    let videoconvert = ElementFactory::make("videoconvert").build()?;

    #[cfg(not(feature = "vaapi"))]
    let format_caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", "NV12")
        .build();

    #[cfg(feature = "vaapi")]
    let videoconvert = ElementFactory::make("vapostproc")
        .property_from_str("scale-method", "fast")
        .build()?;

    #[cfg(feature = "vaapi")]
    let format_caps = {
        let caps_str = "video/x-raw(memory:VAMemory),format=NV12";
        let caps = gstreamer::Caps::from_str(caps_str)?;
        caps
    };

    println!("Format caps: {}", format_caps);
    let format_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &format_caps)
        .build()?;

    // this makes the stream smoother on VAAPI, but on x264enc it causes severe latency
    // especially when the CPU is under load (such as when playing minecraft)
    // #[cfg(feature = "vaapi")]
    // let conversion_queue = ElementFactory::make("queue").build()?;

    #[cfg(all(
        not(feature = "vaapi"),
        any(target_os = "linux", target_os = "windows")
    ))]
    let enc = ElementFactory::make("x264enc")
        //.property("qos", true)
        .property("threads", 4u32)
        .property("aud", false)
        .property("b-adapt", false)
        .property("bframes", 0u32)
        .property("insert-vui", true)
        .property("rc-lookahead", 0)
        .property("vbv-buf-capacity", 120u32)
        .property("sliced-threads", true)
        .property("byte-stream", true)
        .property_from_str("pass", "cbr")
        .property_from_str("speed-preset", "veryfast")
        .property_from_str("tune", "zerolatency")
        .property("bitrate", 250u32)
        .build()?;

    // VideoToolbox H264 encoder
    #[cfg(target_os = "macos")]
    let enc = ElementFactory::make("vtenc_h264_hw")
        //.property("qos", true)
        .property("allow-frame-reordering", false)
        .property("bitrate", 250u32)
        .property("realtime", true)
        .build()?;

    #[cfg(feature = "vaapi")]
    let enc = ElementFactory::make("vah264enc")
        .property("aud", false)
        .property("b-frames", 0u32)
        .property("dct8x8", false)
        .property("key-int-max", 1024u32)
        .property("cpb-size", 120u32)
        .property("num-slices", 4u32)
        .property("ref-frames", 1u32)
        .property("target-usage", 6u32)
        .property_from_str("rate-control", "cbr")
        .property_from_str("mbbrc", "disabled")
        .property("bitrate", 4000u32)
        .build()?;

    println!("Enc: {:?}", enc);

    #[cfg(feature = "vaapi")]
    let h264_caps = gstreamer::Caps::builder("video/x-h264")
        .field("profile", "high")
        .field("stream-format", "byte-stream")
        .build();
    #[cfg(all(
        not(feature = "vaapi"),
        any(target_os = "linux", target_os = "windows")
    ))]
    let h264_caps = gstreamer::Caps::builder("video/x-h264")
        .field("profile", "baseline")
        .field("stream-format", "byte-stream")
        .build();
    let h264_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &h264_caps)
        .build()?;

    let appsink = gstreamer_app::AppSink::builder()
        // Tell the appsink what format we want. It will then be the audiotestsrc's job to
        // provide the format we request.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        .caps(&h264_caps)
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

                // we can .ok() this, because if it DOES fail, the thread will be terminated soon
                buffer_tx.send(packet.to_vec()).ok();
                waker.notify_one();
                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

    // Create the pipeline
    let pipeline = Pipeline::default();

    // Add elements to the pipeline
    pipeline.add_many([
        &src,
        &video_capsfilter,
        &textoverlay,
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
        &textoverlay,
        &videoconvert,
        &format_capsfilter,
        &enc,
        &h264_capsfilter,
        appsink.upcast_ref(),
    ])?;

    // Set the pipeline to playing state
    pipeline.set_state(State::Playing)?;

    let mut stats = StatisticsOverlay::new();

    while let Some(msg) = control_rx.recv().await {
        match msg {
            GStreamerControlMessage::Stop => {
                println!("GStreamer task received termination signal!");
                break;
            }
            GStreamerControlMessage::RequestKeyFrame => {
                println!("Forcing keyframe");
                let force_keyframe_event = gstreamer::Structure::builder("GstForceKeyUnit").build();

                // Send the event to the encoder element
                enc.send_event(gstreamer::event::CustomDownstream::new(
                    force_keyframe_event,
                ));
            }
            GStreamerControlMessage::Bitrate(bitrate) => {
                stats.bitrate = Some(bitrate);
                stats.render_to(&textoverlay);
                //#[cfg(not(feature = "vaapi"))]
                enc.set_property("bitrate", bitrate);
                #[cfg(feature = "vaapi")]
                enc.set_property(
                    "cpb-size",
                    ((bitrate as f64 + 60.0 - 1.0) / 60.0 * 1.5).floor() as u32,
                );
            }
            GStreamerControlMessage::Stats { rtt, loss } => {
                if let Some(rtt) = rtt {
                    stats.rtt = Some(rtt);
                }
                if let Some(loss) = loss {
                    stats.loss = Some(loss);
                }
                stats.render_to(&textoverlay);
            }
        }
    }

    // Clean up
    pipeline.set_state(State::Null)?;

    Ok(())
}
