# Tenebra

Tenebra is a video-streaming server. Connect to Tenebra with a compatible client to view and control another machine's screen.

Tenebra mercator videonis-fluentis est. Iunge ad Tenebram cliente compatibile ut spectes iubeasque machinam aliam.

## Usage

Tenebra uses GStreamer to encode an RTP H.264 stream, so GStreamer's runtime utilities must be installed on your system for Tenebra to work.

Tenebra GStreamer utitur ut faciat RTP H.264 flumen, itaque instrumenta GStreameris installanda sunt, ut Tenebra operet.

[GStreamer Installs](https://gstreamer.freedesktop.org/download/) <- install the RUNTIME version

After the server is built with `cargo build --release`, you may run it:
```
./target/release/tenebra "password" 8080 1366     768 4000
                           ^         ^     ^       ^    ^
                           |         |     |       |    --------.
                     <password>   <port> <width> <height>  <bitrate>
```
