# Development

## Prerequisites

- Rust stable (see `rust-toolchain.toml`).
- **Windows**: MSVC build tools and the Windows SDK (GPUI release builds compile shaders with `fxc.exe`).
- **macOS**: Xcode command line tools.
- **Linux**: `build-essential pkg-config libxkbcommon-x11-dev libwayland-dev libxcb1-dev libfontconfig-dev libfreetype-dev libvulkan-dev libssl-dev libdbus-1-dev` (the full list CI installs is in `.github/workflows/ci.yml`).

## Common commands

```sh
cargo run -p capshot-app                     # debug build (dependencies are optimised)
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p capshot-core --example render_bench
cargo run -p capshot-core --example write_icons -- assets/icons
```

Set `CAPSHOT_TRACE=1` to log canvas bounds, pointer events and tool changes to stderr.

## Cross-compiling from WSL to Windows (no MSVC)

Useful for trying changes on a Windows desktop from Linux. Not used for releases.

1. Install `zig` (0.14) and add shims for `x86_64-w64-mingw32-gcc` (zig cc, `-target x86_64-windows-gnu`, link `-lgpui` with `--whole-archive`, add `-lunwind`, drop `-lgcc*`/`-lmsvcrt`/`-lmingw*`), `x86_64-w64-mingw32-ar`, and an `llvm-rc` shim that runs `zig rc /:output-format coff` and wraps the object in an archive.
2. `rustup target add x86_64-pc-windows-gnu`, then `RC=llvm-rc cargo build -p capshot-app --target x86_64-pc-windows-gnu`.
3. Debug builds compile GPUI shaders at runtime from the crate's source path, which is a Linux path. Start the exe with its working directory on the WSL share (for example `\\wsl.localhost\Ubuntu-24.04\home\<user>`) so that path resolves.

Without `--whole-archive` the manifest resource is dropped and the exe exits with `0xC0000139` before `main`.

### Release builds without the Windows SDK

GPUI's release mode embeds shaders compiled by `fxc.exe`. Without the SDK, compile them on any
Windows machine with `d3dcompiler_47.dll` (part of Windows) through a small tool that calls
`D3DCompileFromFile` with `/O3`-equivalent flags. Write one `const <MODULE>_<VERTEX|FRAGMENT>_BYTES: &[u8] = &[...]`
per stage into `shaders_bytes.rs`. Then build against a local copy of `gpui-pre-windows` whose
`build.rs` copies that file into `OUT_DIR`:

```sh
GPUI_PREBUILT_SHADERS=/path/shaders_bytes.rs RC=llvm-rc cargo build --release -p capshot-app \
  --target x86_64-pc-windows-gnu --config "patch.crates-io.gpui-pre-windows.path='/path/gpui-pre-windows'"
```

The resulting exe is self-contained and starts from any folder.

## Releases

Push a tag `vX.Y.Z` (or `vX.Y.Z-beta.N` for a pre-release). `.github/workflows/release.yml`
builds installers on all three OSes with `cargo-packager` and attaches them to the GitHub release.
Signing uses the repository secrets named in that workflow; without them installers are unsigned.
