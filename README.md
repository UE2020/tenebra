# Tenebra

Tenebra is a video-streaming server written in Rust (you will need a Rust compiler to compile it). Connect to Tenebra with a [compatible client](https://github.com/BlueCannonBall/lux) to view and control another machine's screen.

## Usage

Tenebra uses GStreamer to encode an RTP H.264 stream, so GStreamer's runtime utilities must be installed on your system for Tenebra to work.

[GStreamer Installs](https://gstreamer.freedesktop.org/download/) <- install the RUNTIME version

After the server is built with `cargo build --release`, you may run it:
```
./target/release/tenebra "password" 8080 4000
                           ^         ^    ^
                           |         |    ---|
                     <password>   <port>  <bitrate>
```
