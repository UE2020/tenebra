![image](https://github.com/user-attachments/assets/0811d95b-952c-4f31-828d-6e14b8c2e7a5)

*Two Tenebra instances streaming macOS Ventura and Windows 11 viewed using [Lux Desktop](https://github.com/BlueCannonBall/lux-desktop) on Arch Linux.*

# Tenebra

Tenebra is a remote desktop server based on modern video streaming technology written in Rust. Connect to Tenebra with a [compatible client](https://github.com/BlueCannonBall/lux) to view and control another machine's screen.

## Platform Support

| Platform    | Compatibility |
| --------    | ------------- |
| Linux/X11 | Excellent |
| Mac | Almost excellent (the mouse cursor doesn't automatically appear when the cursor moves, so client-side mouse is a requirement) |
| Windows | Okay (performance may be poor) |
| Linux/Wayland | None; [pipewiresrc](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/4035) [is](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/4137) [too](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/3149) [slow](https://gitlab.freedesktop.org/pipewire/pipewire/-/issues/3910) |

## Usage

Tenebra uses GStreamer to record the screen in a cross-platform way, and to encode H.264 samples, so GStreamer's runtime utilities must be installed on your system for Tenebra to work. On Linux, GStreamer should be available using your package manager. On macOS and Windows, see the hyperlink below.

[GStreamer Installs](https://gstreamer.freedesktop.org/download/)

To use a Github release, you only need the runtime package. To build Tenebra, you need to install both the development and the runtime packages.

After the server is built with `cargo build --release`, you may run it:
```
./target/release/tenebra
```

However, Tenebra reads from a config file which must be populated before running Tenebra. If it is not populated, Tenebra will fail before copying the default config file to the config file directory.

* On **Linux** the config file is at `$XDG_CONFIG_HOME`/tenebra/config.toml or `$HOME`/.config/tenebra/config.toml (e.g. /home/alice/.config/tenebra/config.toml)
* On **Windows** the config file is at `{FOLDERID_RoamingAppData}` (e.g. C:\Users\Alice\AppData\Roaming)
* On **macOS** the config file is at `$HOME`/Library/Application Support (e.g. /Users/Alice/Library/Application Support)

[See the default config file.](src/default.toml)

## Using Hardware Accelerated Encoding (macOS & Linux only)

### VA-API

On Linux, [VA-API](https://en.wikipedia.org/wiki/Video_Acceleration_API) can be used to perform hardware accelerated H.264 encoding. This can be enabled by setting the `hwencode` property in the config.toml to `true`. The `va` GStreamer plugin (NOT the `vaapi` plugin - this one is broken) must be installed and USABLE. If your system supports the `vapostproc` GStreamer element, you may enable the `vapostproc` option as well.

### VideoToolbox

On macOS, [VideoToolbox](https://developer.apple.com/documentation/videotoolbox) can be used to perform hardware accelerated H.264 encoding. This can be enabled by setting the `hwencode` property in the config.toml to `true`. The `vtenc_h264` GStreamer element must be installed and USABLE.

## Touch input

On Linux, Tenebra has support for receiving and emulating touch events (e.g., from an iPad client). The touch emulator is written in C for simplicity and uses uinput.h (this constitutes the only usage of `unsafe` in the project). Using uinput to emulate touch events may require special permissions. Reference your distribution's documentation for details.
