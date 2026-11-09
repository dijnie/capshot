# capshot

Capture, annotate and beautify screenshots. A Rust + [GPUI](https://gpui.rs) rebuild of
[winshot](https://github.com/mrgoonie/winshot) for Windows, macOS and Linux.

## Features

- **Capture**: fullscreen (all displays stitched), region (frozen overlay on every display), window (picker with thumbnails), clipboard, file import, drag and drop, `capshot <image>`.
- **Annotate**: rectangle, ellipse, tapered arrow (straight or curved), line, text, numbered markers, spotlight. Move, resize, undo/redo (50 steps), keyboard shortcuts as in winshot (V R E A L T N S C, Ctrl+Z/Y, Del).
- **Crop**: non-destructive, with Free/16:9/4:3/1:1/9:16/3:4 presets. Annotations stay attached to the image.
- **Beautify**: padding, corner radius, drop shadow, inset, border (outside/center/inside), 24 gradient presets, solid colours, or the screenshot's own edge colour; output ratios Auto/1:1/4:3/3:2/16:9/5:3/9:16/3:4/2:3.
- **Export**: PNG or JPEG (quality), Save As, Quick Save (timestamp/date/number names), copy to clipboard, auto-copy after save.
- **Shell**: tray icon and menu, global hotkeys (winshot's defaults and format), one instance per user (`capshot --region` from a script or desktop shortcut reaches the running app), close to tray, start in tray, launch at login, save notifications.
- **Library**: screenshots in the quick-save folder, newest first; open, show in folder, delete to the trash.
- **Cloud**: Cloudflare R2 and Google Drive (your own OAuth client). Secrets are kept in the OS keychain.
- **Updates**: optional check against GitHub releases.

Existing winshot settings (`%APPDATA%\WinShot\config.json`) are imported on first run; the winshot file is never modified.

## Command line

```text
capshot [--fullscreen | --region | --window] [IMAGE]
```

## Platform notes

- **macOS**: grant Screen Recording (and Accessibility for global shortcuts) in System Settings → Privacy & Security.
- **Linux, Wayland**: global hotkeys and region overlays depend on the desktop's portals. Where they are missing, bind `capshot --region` to a desktop shortcut.

## Building

See [docs/development.md](docs/development.md). In short, with Rust stable and the platform's
GPUI prerequisites:

```sh
cargo run -p capshot-app --release
cargo test --workspace
```

## Credits

- Derived from [winshot](https://github.com/mrgoonie/winshot) by mrgoonie (BSD 3-Clause).
- UI: [GPUI](https://github.com/zed-industries/zed) and [GPUI Kit / gpui-component](https://github.com/longbridge/gpui-kit).
- Bundled font: Inter (SIL Open Font License 1.1, `assets/fonts/OFL.txt`). Icons: Lucide (ISC).

## License

BSD 3-Clause. See [LICENSE](LICENSE).
