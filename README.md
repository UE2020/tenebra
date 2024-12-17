![image](https://github.com/user-attachments/assets/0811d95b-952c-4f31-828d-6e14b8c2e7a5)

*Two Tenebra instances streaming macOS Ventura and Windows 11 viewed using [Lux Desktop](https://github.com/BlueCannonBall/lux-desktop) on Arch Linux.*

# Tenebra

Tenebra is a remote desktop server based on modern video streaming technology written in Rust (you will need a Rust compiler to compile it). Connect to Tenebra with a [compatible client](https://github.com/BlueCannonBall/lux) to view and control another machine's screen.

## Platform Support

| Platform    | Compatibility |
| --------    | ------------- |
| Linux/X11 | Excellent |
| Mac | Almost excellent (the mouse cursor doesn't automatically appear when the cursor moves, so client-side mouse is a requirement) |
| Windows | Okay (performance may be poor) |
| Linux/Wayland | None; pipewiresrc is too slow |

## Usage

Tenebra uses GStreamer to encode an RTP H.264 stream, so GStreamer's runtime utilities must be installed on your system for Tenebra to work. GStreamer should be available using your package manager. On macOS, reference the hyperlink below.

[GStreamer Installs](https://gstreamer.freedesktop.org/download/) <- install the development and runtime libraries

**You need an SSL certificate to run tenebra. Before building, obtain an SSL cert and place `cert.pem` and `key.pem` in the root directory.**

After the server is built with `cargo build --release`, you may run it:
```
./target/release/tenebra "password" 8080 4000
                           ^         ^    ^
                           |         |    ---|
                     <password>   <port>  <bitrate (optional)>
```

## Using UPnP

Tenebra can portforward its built-in signalling server automatically using the UPnP (Universal Plug N Play) protocol. This can be achieved by compiling with the `upnp` feature flag. Do not use UPnP if you have already added a manual portforwarding rule.

### Common issues

#### UPnP portforwarding rule disappeares after a while

Some routers automatically clean up unused UPnP portforwarding rules. In this case, this is harmful because Tenebra cleans up its own rules when it's stopped, and because the signalling server may run for a very long time without receiving any requests. On my Verizon Fios router I was able to disable this functionality by unticking the box under "Advanced" > "Universal Plug and Play" > "Enable Automatic Cleanup of Old Unused UPnP Services".

#### UPnP portforwarding rule exists, but does not work

The UPnP portforwarding rule is overrided by any existing manual rule for the signalling server's port. Remove any conflicting manually added rules, or just disable the `upnp` feature flag to stop using UPnP.

## Using VA-API

On Linux, VA-API can be used to perform hardware accelerated H.264 encoding. This can be enabled by compiling with the `vaapi` feature flag. The `va` GStreamer plugin (NOT the `vaapi` plugin - this one is broken) must be installed and usable.

## Touch input

On Linux, Tenebra has support for receiving and emulating touch events (e.g., from an iPad client). The touch emulator is written in C for simplicity and uses uinput.h (this constitutes the only usage of `unsafe` in the project). Using uinput to emulate touch events may require special permissions. Reference your distribution's documentation for details.
