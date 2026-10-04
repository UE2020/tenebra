# Web client assets

The files in this directory are the static [Lux](https://github.com/BlueCannonBall/lux) web
client, vendored verbatim so that the Tenebra server can serve a user-facing client from
its own origin.

- Upstream: https://github.com/BlueCannonBall/lux
- Commit: `f296b4387d9361ab802ba32a184f90cf46a275f1`
- License: GNU Affero General Public License v3.0 (see the repository's `LICENSE`)

Vendored files: `index.html`, `index.js`, `service-worker.js`, `manifest.json`,
`pico.classless.min.css`, `favicon.ico`, `icon.png`, `apple-touch-icon.png`, `mouse.png`.

Upstream `screenshot.png` is not vendored: it is not referenced at runtime.

To update, re-download the above files from the pinned commit and refresh `.upstream-sha`.
