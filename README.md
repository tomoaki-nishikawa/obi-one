# obi-one

Rust + Tauri implementation of the maisoku obi replacement tool.

## Current Scope

- Standalone Tauri desktop app shell
- Multiple PDF queue UI
- Output directory setting
- Automatic `yyyymmdd` output folder planning
- Processed PDF / original-flyer JPEG output options
- Preset template PDF
- Bundled PDFium dynamic library
- Custom obi band image setting
- Rust image composition core
- Unit tests for output naming, settings, image composition, PDFium rendering, and PDF/JPEG saving

## Development

```sh
cd src-tauri
cargo test --lib --bins
cargo build
```

The UI is static HTML/CSS/JS under `ui/`; no frontend build step is required.

## Runtime PDF Dependency

PDF rendering is isolated in `src-tauri/src/processor.rs` and currently uses `pdfium-render`.

PDFium is bundled as a Tauri resource under:

```text
src-tauri/resources/pdfium/
```

Install the current platform's dynamic PDFium library with:

```sh
./scripts/fetch-pdfium.sh
```

Install universal macOS (arm64 + x86_64) and Windows x64 PDFium libraries with:

```sh
./scripts/fetch-pdfium.sh all
```

The app looks for PDFium in this order:

1. Tauri resource `pdfium/<platform-library-name>`
2. `src-tauri/resources/pdfium/<platform-library-name>` during local development
3. The executable's current working directory
4. The operating system library search path

The bundled binaries are downloaded from the `bblanchon/pdfium-binaries` GitHub releases.

## Packaging

macOS builds can be created on macOS.

The release macOS artifact is a universal app for both Apple Silicon and Intel Macs. Its bundled `libpdfium.dylib` is also universal. The workflow applies an ad-hoc signature to the app executable and PDFium, then produces a DMG and a ZIP that preserves executable permissions. Because it is not signed with an Apple Developer ID or notarized, users must allow the first launch in macOS Privacy & Security settings.

Windows standalone builds should be created on Windows, either on a Windows PC or a Windows CI runner such as GitHub Actions. This is the recommended path for this project because Tauri officially supports the Windows MSVC target, and GitHub's `windows-latest` runner already has the Windows toolchain needed by the Tauri bundler.

This repository can carry both PDFium dynamic libraries under `src-tauri/resources/pdfium/`. The Windows build must include `src-tauri/resources/pdfium/pdfium.dll`; the macOS build must include `src-tauri/resources/pdfium/libpdfium.dylib`.

Local platform build:

```sh
npm ci
./scripts/fetch-pdfium.sh current
npm run build
```

macOS universal release build:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/fetch-pdfium.sh mac-universal
npm run build -- --target universal-apple-darwin --bundles app --no-sign
./scripts/package-macos.sh
```

The macOS release files are written to `dist/macos/`.

Cross-platform release build:

- Push the project to GitHub.
- Open GitHub Actions and run `Build obi-one` manually, or push to `main`.
- Download `obi-one-macos-universal` for the universal DMG/ZIP and installation notes.
- Download `obi-one-windows` from the workflow artifacts.
- The Windows artifact contains the NSIS setup executable and MSI installer from:
  - `src-tauri/target/release/bundle/nsis/**/*-setup.exe`
  - `src-tauri/target/release/bundle/msi/**/*.msi`

GitHub Actions is the preferred Windows build command:

```sh
npm ci
./scripts/fetch-pdfium.sh current
cd src-tauri
cargo test --lib --bins
cd ..
npm run build -- --bundles nsis,msi
```

### macOS to Windows Cross Build

Tauri can cross-compile Windows NSIS installers from macOS with `cargo-xwin`, but this is a fallback path. It requires extra local tools and only produces the NSIS installer, not MSI.

Required tools:

```sh
brew install llvm nsis
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
```

Build command:

```sh
npm ci
./scripts/fetch-pdfium.sh windows-x64
npm run build -- --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
```

The cross-build output is written under:

```text
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/
```

Use GitHub Actions if the local cross-build fails or if an MSI is required.

## Notes

`cargo test` without arguments tries to run doctests. In this local environment, `rustdoc` is not installed, so use:

```sh
cargo test --lib --bins
```
