![image](https://github.com/user-attachments/assets/0811d95b-952c-4f31-828d-6e14b8c2e7a5)

*Two Tenebra instances streaming macOS Ventura and Windows 11 viewed using [Lux Desktop](https://github.com/BlueCannonBall/lux-desktop) on Arch Linux.*

# Tenebra

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/UE2020/tenebra)

Tenebra is a remote desktop server based on modern video streaming technology written in Rust. Connect to Tenebra with a [compatible client](https://github.com/BlueCannonBall/lux) to view and control another machine's screen.

## Platform Support

| Platform    | Compatibility |
| --------    | ------------- |
| Linux/X11 | Excellent |
| Mac | Good[1] |
| Windows | Excellent |
| Linux/Wayland | None; [pipewiresrc](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/4035) [is](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/4137) [too](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/3149) [slow](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/3910) |

[1] Latency seems to be higher. Sound forwarding and pen/touch input are not implemented.

## Usage

Tenebra uses GStreamer to record the screen in a cross-platform way, and to encode H.264 samples, so GStreamer's runtime utilities must be installed on your system for Tenebra to work. On Linux, GStreamer should be available using your package manager. On macOS and Windows, see the hyperlink below.

[GStreamer Installs](https://gstreamer.freedesktop.org/download/)

To use a Github release, you only need the runtime package. To build Tenebra, you need to install both the development and the runtime packages. On Windows, GStreamer's bin folder must be added to the PATH.

After the server is built with `cargo build --release`, you may run it. On macOS and Linux, this is as easy as:
```
./target/release/tenebra
```

But on Windows, Tenebra must run as a service in order to have the necessary integrity level to interact with all parts of the desktop. First, a service must be registered:
```
sc create Tenebra binPath= "C:\path\to\tenebra\exe"
```

Then, starting Tenebra is as easy as:
```
sc start Tenebra
```

On first start, Tenebra writes its config file automatically with a randomly generated password and a free port, and keeps running; no manual editing is required. If the password is missing or still set to `"placeholder"`, Tenebra generates one and rewrites just that line.

* On **Linux** the config file is at `$XDG_CONFIG_HOME`/tenebra/config.toml or `$HOME`/.config/tenebra/config.toml (e.g. /home/alice/.config/tenebra/config.toml)
* On **Windows** the config file is at C:\tenebra\config.toml (the directory is created automatically)
* On **macOS** the config file is at `$HOME`/Library/Application Support (e.g. /Users/Alice/Library/Application Support)

Every key in the config file is optional and falls back to a default. Unknown keys, conflicting options (`full_chroma` together with `hwencode`, or `vapostproc` without `hwencode`), and a missing `cert`/`key` under `tls = "custom"` produce an explanatory error instead of an opaque parse failure.

Tenebra also accepts a few command-line flags:

* `tenebra --init` writes a fresh config file if none exists, and never overwrites an existing one.
* `tenebra --version` prints the version.
* `tenebra --console` (Windows only) runs Tenebra in the current terminal instead of as a service.

[See the default config file.](src/default.toml)

The server hosts the user-facing web client itself: the vendored [Lux](https://github.com/BlueCannonBall/lux) is embedded in the binary and served at `https://<host>:<port>/`, so no separate static site is needed to connect. The plain-text configuration summary is at `/info`.

Alternatively, use [Tenebra GTK](https://github.com/BlueCannonBall/tenebra-gtk) to configure Tenebra in a user-friendly way:

![image](https://github.com/user-attachments/assets/be8aa60a-b19e-4b1a-82cb-d41e613cf82c)

## TLS

The `tls` key in the config file controls how Tenebra secures connections:

* `tls = "auto"` (default): Tenebra generates a self-signed certificate (`cert.pem`/`key.pem` in the config directory), reuses it on later starts, and prints its SHA-256 fingerprint at startup. No domain or Let's Encrypt certificate is needed; accept the certificate once in the browser. The web client and the signalling endpoint share the same origin, so accepting it once is enough.
* `tls = "off"`: plain HTTP, for native clients on a trusted LAN only. The bundled web client requires HTTPS.
* `tls = "custom"`: use the certificate and key files at the `cert` and `key` paths, for a certificate obtained elsewhere (e.g. behind a reverse proxy).

## Using Hardware Accelerated Encoding (All Platforms)

### VA-API

On Linux, [VA-API](https://en.wikipedia.org/wiki/Video_Acceleration_API) can be used to perform hardware accelerated H.264 encoding. This can be enabled by setting the `hwencode` property in the config.toml to `true`. The `va` GStreamer plugin (NOT the `vaapi` plugin - this one is broken) must be installed and USABLE. If your system supports the `vapostproc` GStreamer element, you may enable the `vapostproc` option as well.

### VideoToolbox

On macOS, [VideoToolbox](https://developer.apple.com/documentation/videotoolbox) can be used to perform hardware accelerated H.264 encoding. This can be enabled by setting the `hwencode` property in the config.toml to `true`. The `vtenc_h264` GStreamer element must be installed and USABLE.

### Media Foundation Encoder

On Windows, [Media Foundation](https://learn.microsoft.com/en-us/windows/win32/medfound/microsoft-media-foundation-sdk) can be used to perform hardware accelerated H.264 encoding. This can be enabled by setting the `hwencode` property in the config.toml to `true`. The `mfh264enc` GStreamer element must be installed and USABLE. Enable `hwencode` will also automatically enable the use of D3D11 for video format conversion.

## Touch input & pen input

On Linux and Windows, Tenebra has support for receiving and emulating touch and pen events (e.g. from an iPad client).

On Linux, this requires permission to access uinput. Reference your distribution's documentation for details.
