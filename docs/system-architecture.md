# System architecture

## Crates

```text
capshot-app       GPUI app: windows, editor view, overlays, dialogs        (depends on all below)
capshot-cloud     R2, Google Drive, keychain secrets, update check          (core)
capshot-platform  capture, clipboard, hotkeys, tray, single instance,
                  autostart, notifications, trash                           (core)
capshot-core      document model, compositor, export, config, editing rules (no UI, no OS)
```

`capshot-core` has no GPUI or OS dependency, so its tests run anywhere.

## One compositor for preview and export

`capshot_core::render::render(doc, RenderOptions { scale, .. })` draws a document with tiny-skia.
The editor calls it at the view scale and shows the result as a GPUI image; Save, Quick Save,
Copy and uploads call it at scale 1.0. GPUI paints only interactive chrome over the preview
(selection outline, grips, crop frame), so exports always match what the user saw.

Layer order follows winshot's Konva stage: background (colour, two-stop or multi-stop gradient,
or cover-scaled image) → inset background → drop shadow (blurred at reduced resolution) →
rounded-clipped screenshot → border → annotations → spotlight dimming.

Performance (release, x86-64): 4K document with shadow and annotations, 86 ms at 1.0; 8.5 ms at
0.25 thanks to a cached chain of half-size copies of the screenshot.

## Coordinates

- **Original pixels**: annotations and the crop rectangle. Cropping changes what is visible,
  never the annotation data, so re-cropping and undo are lossless.
- **Canvas pixels**: the output image (padding, ratio and inset applied); `Layout` maps between
  the two.
- **Screen pixels**: GPUI logical pixels; `ViewTransform` fits the canvas into the view with zoom
  and pan.

## Editor state

`capshot-app/src/editor.rs` holds the document, an undo history of `Content` (annotations + crop),
the active tool and drag state. It has no GPUI dependency and is unit-tested. Shape creation,
grips and hit-testing live in `capshot_core::edit`.

## Shell

`shell.rs` owns the main window, tray, hotkey service and single-instance channel. OS callbacks
send commands over channels that a GPUI task drains every 80 ms. Closing the window to the tray
removes the window but keeps the `MainView` entity, so reopening restores the session.

## Config

JSON with winshot's camelCase keys at the platform config directory (`.../capshot/config.json`).
Secrets are never written there; they go to the OS keychain under the service `capshot`.
