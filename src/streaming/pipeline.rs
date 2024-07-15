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

use gstreamer::prelude::*;
use gstreamer::{element_error, ElementFactory, Pipeline, State};

use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::UnboundedSender;

pub fn start_pipeline(
    bitrate: u32,
    startx: u32,
    show_mouse: bool,
    mut done: Receiver<()>,
    buffer_tx: UnboundedSender<Vec<u8>>,
    ulp_pt: u8,
    h264_pt: u8,
) {
    #[cfg(target_os = "linux")]
    let src = ElementFactory::make("ximagesrc")
        .property("use-damage", false)
        .property("startx", startx)
        .property("show-pointer", show_mouse)
        .property("blocksize", 16384u32)
        .property("remote", true)
        .build()
        .unwrap();

    #[cfg(target_os = "macos")]
    let src = ElementFactory::make("avfvideosrc")
        .property("capture-screen", true)
        .property("capture-screen-cursor", show_mouse)
        .build()
        .unwrap();

    #[cfg(target_os = "windows")]
    let src = ElementFactory::make("d3d12screencapturesrc")
        .property("show-cursor", show_mouse)
        .build()
        .unwrap();

    let video_caps = gstreamer::Caps::builder("video/x-raw")
        .field("framerate", gstreamer::Fraction::new(60, 1))
        .build();
    let video_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &video_caps)
        .build()
        .unwrap();

    let videoconvert = ElementFactory::make("videoconvert").build().unwrap();

    let format_caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", "NV12")
        .build();
    let format_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &format_caps)
        .build()
        .unwrap();

    let enc = ElementFactory::make("x264enc")
        .property("qos", true)
        .property("threads", &4u32)
        .property("aud", true)
        .property("b-adapt", false)
        .property("key-int-max", 512u32)
        .property("bframes", 0u32)
        .property("insert-vui", true)
        .property("rc-lookahead", 0)
        .property("vbv-buf-capacity", 120u32)
        .property("sliced-threads", true)
        .property("byte-stream", true)
        .property_from_str("pass", "cbr")
        .property_from_str("speed-preset", "veryfast")
        .property_from_str("tune", "zerolatency")
        .property("bitrate", bitrate)
        .build()
        .unwrap();

    // let h264_caps = gstreamer::Caps::builder("video/x-h264")
    //     .field("profile", "baseline")
    //     .field("stream-format", "byte-stream")
    //     .build();
    // let h264_capsfilter = ElementFactory::make("capsfilter")
    //     .property("caps", &h264_caps)
    //     .build()
    //     .unwrap();

    let rtph264pay = ElementFactory::make("rtph264pay")
        .property("mtu", &1000u32)
        .property("pt", h264_pt as u32)
        .property_from_str("aggregate-mode", "zero-latency")
        .property("config-interval", -1)
        .build()
        .unwrap();

    let fecenc = ElementFactory::make("rtpulpfecenc")
        .property("pt", ulp_pt as u32)
        //.property("multipacket", true)
        .property("percentage", 100u32)
        .build()
        .unwrap();

    let redenc = ElementFactory::make("rtpredenc")
        // this doesn't actually matter because webrtc-rs rewrites the pt and ssrc
        .property("pt", 112i32)
        .property("allow-no-red-blocks", true)
        //.property("distance", 2u32)
        .build()
        .unwrap();

    // let rtp_caps = gstreamer::Caps::builder("application/x-rtp")
    //     .field("media", "video")
    //     .field("clock-rate", 90000)
    //     .field("encoding-name", "H264")
    //     .field("payload", 102)
    //     .field("rtcp-fb-nack-pli", true)
    //     .field("rtcp-fb-ccm-fir", true)
    //     .field("rtcp-fb-x-gstreamer-fir-as-repair", true)
    //     .build();
    // let rtp_capsfilter = ElementFactory::make("capsfilter")
    //     .property("caps", &rtp_caps)
    //     .build()
    //     .unwrap();

    let appsink = gstreamer_app::AppSink::builder()
        // Tell the appsink what format we want. It will then be the audiotestsrc's job to
        // provide the format we request.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        .caps(&gstreamer::Caps::builder("application/x-rtp").build())
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

                buffer_tx.send(packet.to_vec()).ok();

                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

    // Create the pipeline
    let pipeline = Pipeline::default();

    // Add elements to the pipeline
    pipeline
        .add_many([
            &src,
            &video_capsfilter,
            &videoconvert,
            &format_capsfilter,
            &enc,
            //&h264_capsfilter,
            &rtph264pay,
            //&rtp_capsfilter,
            &fecenc,
            &redenc,
            appsink.upcast_ref(),
        ])
        .unwrap();
    gstreamer::Element::link_many([
        &src,
        &video_capsfilter,
        &videoconvert,
        &format_capsfilter,
        &enc,
        //&h264_capsfilter,
        &rtph264pay,
        //&rtp_capsfilter,
        &fecenc,
        &redenc,
        appsink.upcast_ref(),
    ])
    .unwrap();

    // Set the pipeline to playing state
    pipeline.set_state(State::Playing).unwrap();

    // Wait until error or EOS
    let bus = pipeline.bus().unwrap();
    loop {
        let msg = bus.timed_pop(gstreamer::ClockTime::SECOND);
        if done.try_recv().is_ok() {
            println!("GStreamer thread received termination signal!");
            break;
        }
        if let Some(msg) = msg {
            use gstreamer::MessageView;
            match msg.view() {
                MessageView::Eos(..) => break,
                MessageView::Error(err) => {
                    eprintln!(
                        "Error received from element {:?}: {}",
                        err.src().map(|s| s.path_string()),
                        err.error()
                    );
                    eprintln!("Debugging information: {:?}", err.debug());
                    break;
                }
                _ => (),
            }
        }
    }

    // Clean up
    pipeline.set_state(State::Null).unwrap();
}
