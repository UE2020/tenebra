use gstreamer::prelude::*;
use gstreamer::{ElementFactory, Pipeline, State, element_error};
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::UnboundedSender;

pub fn start_pipeline(
    bitrate: u32,
    startx: u32,
    show_mouse: bool,
    mut done: Receiver<()>,
    buffer_tx: UnboundedSender<Vec<u8>>,
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

    let x264enc = ElementFactory::make("x264enc")
        .property("qos", true)
        .property("threads", &4u32)
        .property("aud", true)
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
        .property("bitrate", bitrate)
        .build()
        .unwrap();

    let h264_caps = gstreamer::Caps::builder("video/x-h264")
        .field("profile", "baseline")
        .field("stream-format", "byte-stream")
        .build();
    let h264_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &h264_caps)
        .build()
        .unwrap();

    let rtph264pay = ElementFactory::make("rtph264pay")
        .property("mtu", &1000u32)
        .property_from_str("aggregate-mode", "zero-latency")
        .property("config-interval", -1)
        .build()
        .unwrap();

    let rtp_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("clock-rate", 90000)
        .field("encoding-name", "H264")
        .field("payload", 97)
        .field("rtcp-fb-nack-pli", true)
        .field("rtcp-fb-ccm-fir", true)
        .field("rtcp-fb-x-gstreamer-fir-as-repair", true)
        .build();
    let rtp_capsfilter = ElementFactory::make("capsfilter")
        .property("caps", &rtp_caps)
        .build()
        .unwrap();

    let appsink = gstreamer_app::AppSink::builder()
        // Tell the appsink what format we want. It will then be the audiotestsrc's job to
        // provide the format we request.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        .caps(&rtp_caps)
        .build();

    // appsink callback - send rtp packets to the streaming thread
    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            // Add a handler to the "new-sample" signal.
            .new_sample(move |appsink| {
                // Pull the sample in question out of the appsink's buffer.
                let sample = appsink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;
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
            &x264enc,
            &h264_capsfilter,
            &rtph264pay,
            &rtp_capsfilter,
            appsink.upcast_ref(),
        ])
        .unwrap();
    gstreamer::Element::link_many([
        &src,
        &video_capsfilter,
        &videoconvert,
        &format_capsfilter,
        &x264enc,
        &h264_capsfilter,
        &rtph264pay,
        &rtp_capsfilter,
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
