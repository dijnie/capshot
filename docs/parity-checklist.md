# winshot parity checklist

Status of each winshot 1.6.0 feature in capshot.

- **Verified (Win)**: exercised in the running app on Windows 11 (3 monitors, 150% scale).
- **Unit-tested**: logic covered by automated tests; not yet driven in the UI.
- **Implemented**: code exists and compiles for Windows; not yet exercised.
- **Pending**: not done.

macOS and Linux builds have not been run yet. The Linux-only code (ksni tray) has not been compiled; CI is the first check.

## Capture

| Feature | Status |
| --- | --- |
| Region (overlay on every display, W×H label, Esc/right-click cancel) | Verified (Win) |
| Window (picker with thumbnails) | Verified (Win) |
| Fullscreen (all displays stitched) | Unit-tested (stitching); Implemented |
| Clipboard image / copied image file | Implemented |
| Import file dialog, drag and drop, `capshot <image>` | `capshot <image>` Verified (Win); others Implemented |

## Editor

| Feature | Status |
| --- | --- |
| Rectangle, arrow, number, text tools | Verified (Win) |
| Ellipse, line, spotlight, curved arrow | Unit-tested (render, editing) |
| Select, move, resize grips, Shift constraints | Unit-tested |
| Undo/redo (50 steps), delete, Esc | Unit-tested |
| Tool shortcuts V R E A L T N S C | Verified (Win) for R A N T C |
| Crop with handles, apply, cancel, reset, aspect presets | Verified (Win) for drag + apply; others Unit-tested |
| Padding, radius, shadow, inset, border, gradients, solid, edge colour, output ratio | Unit-tested (compositor); panel shown on Windows |
| Zoom and pan | Implemented |

## Export

| Feature | Status |
| --- | --- |
| Quick Save (PNG, naming patterns) | Verified (Win); patterns Unit-tested |
| JPEG with quality | Unit-tested (encoder) |
| Save As dialog, Copy, auto-copy after save | Implemented |
| Include background setting | Implemented (winshot stored it but never applied it; capshot applies it) |

## Shell

| Feature | Status |
| --- | --- |
| Single instance, argument forwarding | Verified (Win) |
| Global hotkeys (winshot format and defaults) | Parsing Unit-tested; registration Verified (Win): conflicts are reported (winshot held the keys during the test) |
| Tray icon and menu | Implemented (Win/macOS); Linux Pending compile |
| Close to tray, start in tray | Implemented |
| Launch at login | Implemented |
| Save notification | Implemented |
| Settings dialog (hotkeys recorder, startup, quick save, export, cloud, about) | Implemented |
| winshot config import | Verified (Win) on a real winshot config |

## Library, cloud, updates

| Feature | Status |
| --- | --- |
| Library grid, open, show in folder, delete to trash | Implemented; scan and thumbnail size Unit-tested |
| Cloudflare R2 upload, test connection, public URL | Unit-tested against a local mock |
| Google Drive sign-in (PKCE), upload, sharing link, sign-out | Unit-tested against a local mock |
| Secrets in OS keychain | Implemented |
| Update check, skip version | Unit-tested; off unless built with `CAPSHOT_UPDATE_REPO` |

## Packaging

| Feature | Status |
| --- | --- |
| CI (fmt, clippy, tests on 3 OSes) | Written; not yet run |
| Installers (NSIS/MSI, DMG, deb/AppImage) via cargo-packager | Written; not yet run |
| Code signing | Workflow steps present; needs certificates |

## Deliberate differences from winshot

- Annotations are stored in image pixels, so changing padding or ratio no longer shifts them, and cropping never deletes them.
- winshot's Transformer showed a rotate handle but never saved rotation; capshot has no rotation.
- Quick Save names use the `capshot_` prefix.
- winshot's Windows Credential Manager secrets are not imported; enter R2 and Drive keys again.
