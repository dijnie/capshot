# Dependencies

Why each major dependency is used, and the version rules.

## GPUI and gpui-component

- `gpui-component` is pinned to `=0.9.0`.
- gpui-component 0.9.0 depends on `gpui-pre =0.3.11`, a crates.io snapshot of Zed's gpui published by the gpui-component maintainer. The workspace imports it as `gpui = { package = "gpui-pre", version = "=0.3.11" }`, so code says `use gpui::...`.
- Upgrade both together, and only to a pair where gpui-component's own `Cargo.toml` names the gpui version.

## Rendering

- `tiny-skia`: CPU rasteriser for the compositor in `capshot-core`. Preview and export use the same code, so they match. GPUI cannot render offscreen into an image.
- `cosmic-text`: text shaping and rasterising. Inter (OFL, `assets/fonts/`) is bundled as the fallback font.

## Platform

| Concern | Crate | Notes |
| --- | --- | --- |
| Screen and window capture | `xcap` | Windows, macOS, Linux X11 and Wayland (portal) |
| Clipboard | `arboard` | `wayland-data-control` feature for Wayland |
| Global hotkeys | `global-hotkey` | Windows, macOS, X11. Wayland uses the GlobalShortcuts portal through `ashpd` |
| Tray | `tray-icon` (Windows, macOS), `ksni` (Linux) | ksni talks D-Bus StatusNotifierItem, so no GTK main loop competes with GPUI |
| Autostart | `auto-launch` | |
| Notifications | `notify-rust` | |

## Cloud

| Concern | Crate |
| --- | --- |
| Secrets | `keyring` |
| HTTP | `reqwest` with rustls |
| R2 (S3 API) | `rusty-s3` presigned requests |
| Google Drive OAuth | `oauth2` |
| Update check | `semver` |
