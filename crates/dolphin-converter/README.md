# dolphin-converter

High-performance native parallel file converter integrated into Dolphin file manager (KDE / KF6).

## Architecture
- Service Menus: `~/.local/share/kio/servicemenus/converter-*.desktop`
- AI Removal: `silueta.onnx` via ONNX Runtime dynamic loader (`/usr/lib/libonnxruntime.so`).
- Parallelism: Rayon thread pool for multi-file processing; sequential execution for AI & video.
- Safety: Atomic reservation (`create_new`), isolated LibreOffice profiles in `/tmp`.

## Build & Deploy
```bash
cargo build --release
cp target/release/dolphin-convert ~/.local/bin/
kbuildsycoca6
cargo clean
```
