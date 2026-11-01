# Contributing

Fork the repository, branch from `main`, and open a focused pull request. For larger changes, discuss the intended behavior in an issue first.

## Set up and run

Install Rust stable and Node.js 24 LTS with npm. Node runs the Tauri CLI and frontend tests; the application has no Node.js runtime.

Follow the [Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/). Windows needs Visual Studio C++ Build Tools, Rust MSVC and WebView2. macOS needs Xcode command-line tools. On Ubuntu/Debian:

```sh
sudo apt update
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libxdo-dev libssl-dev librsvg2-dev libdbus-1-dev patchelf
```

Then:

```sh
npm ci
npm start
```

The first start compiles the native backend and opens a development window. Rust changes rebuild the app. Frontend assets are under `desktop/`; restart the development command if an asset change is not reflected. Use a local Windows checkout rather than a WSL UNC share for the normal Windows toolchain.

## Architecture

| Path | Responsibility |
| --- | --- |
| `src/desktop_main.rs` | Tauri window, event bridge and background command dispatch |
| `src/backend.rs` | Serialized application service, profiles, settings, cache and package operations |
| `src/inventory.rs` | Client discovery and TOC scanning |
| `src/archive.rs` | ZIP paths, compatibility, backups, staging and rollback |
| `src/provider.rs` | CurseForge API, releases, downloads, hashes and update assessment |
| `src/grouping.rs` | Package ownership and manifests |
| `src/images.rs` | Local image decoding, including TGA and BLP2 DXT |
| `src/migration.rs` | One-time native Windows credential migration |
| `desktop/bridge.js` | Narrow Tauri command API for the renderer |
| `desktop/renderer.js`, `index.html`, `styles.css` | UI |
| `desktop/grouping.js`, `bundles.json` | Display grouping and known bundle layouts |
| `capabilities/`, `tauri*.conf.json` | Native permissions and packaging |
| `tests/`, inline Rust tests | Frontend and backend verification |

All operations use Rust directly; there is no sidecar executable or Electron runtime. The optional `wow-addons-core` CLI is a debugging tool that calls the same library.

The frontend cannot fetch arbitrary URLs or use a filesystem plugin. The native service validates commands, URL hosts, paths and installer plans. Navigation is restricted to the local UI. Preserve this boundary when adding features.

## Checks

```sh
npm run check
npm test
cargo test --locked --lib
cargo check --locked --features desktop --bin wow-addons
```

Do not weaken archive validation to accommodate an addon. Normalize legitimate input, then retain traversal, symlink, collision, size and compatibility checks. Use synthetic clients under temporary directories for installation/removal tests.

On Windows, build a debug app and run the native UI smoke test:

```sh
cargo build --locked --features desktop --bin wow-addons
npm run test:ui
```

The test creates a separate profile and game, tests install/reinstall/remove, and verifies saved keys across a restart. Its test key is isolated by profile and cleared afterward. Set `WOW_ADDONS_BINARY` to a different debug executable if needed. Release builds ignore the `WOW_ADDONS_TEST_DATA` override unless explicitly compiled with the non-release `test-driver` feature.

`npm run test:live` is an optional Windows migration/update test. When the previous encrypted preview key and Local State are available, it copies those encrypted files to a temporary profile, migrates using native Windows APIs, installs Bagnon, selects an older stable file and checks for an update. It never writes to the real game or logs the key, and clears its isolated credential afterward. It needs network access and compatible historical releases.

## Package

```sh
npm run dist
```

Build on the target OS and architecture. Outputs are in `target/release/` and `target/release/bundle/`. Windows NSIS installs WebView2 when needed; a portable EXE requires it already installed. macOS and Linux builds are supported from source, with full game-management validation still pending.

`npm run build:icons` regenerates committed packaging icons from `desktop/assets/app-icon.png`. Preserve the license and revision of vendored Phosphor SVGs.

## Pull requests and commits

Explain the problem, resulting behavior and validation. Include a screenshot for UI changes and identify platform limitations. Keep startup/client switching offline; downloads and API work belong to explicit actions.

Use short Conventional Commits and small logical batches:

```text
feat: add release picker
fix: accept Windows ZIP paths
docs: explain API setup
chore: update build tools
```

Do not combine unrelated changes or add co-author trailers. Squash-merged PR titles must also follow this format: `fix:` bumps patch, `feat:` minor, and a breaking change uses `feat!:` or a `BREAKING CHANGE:` footer.

Never commit credentials, API response captures, caches, game data, ZIPs, compiled binaries or local development tools. `.gitignore` adapts the GitHub Node and Rust templates and includes these project-specific exclusions. Commit both lockfiles.

## Releases

The initial app version is **0.1.0**. Release Please maintains a release PR with the next SemVer, changelog and synchronized versions in npm, Cargo and Tauri configuration. Normal feature PRs do not edit versions manually.

Merge the release PR when ready. The same workflow creates a tag and draft release, builds the tagged source on Windows x64, attaches the installer, portable EXE and SHA256 checksums, and publishes only after upload succeeds. It does not depend on a tag event emitted by `GITHUB_TOKEN`.

One-time repository setup: **Settings → Actions → General → Workflow permissions → Allow GitHub Actions to create and approve pull requests**. No CurseForge secret or release PAT is required. If protected branches require CI on bot-created PRs, manually run CI on the release PR branch, or configure a scoped GitHub App token to trigger PR checks.

A failed build leaves the release as a draft. Use **Release → Run workflow** with its existing `vX.Y.Z` tag to retry the same source. A source correction needs a new release; never move a published tag. Signing/notarization and publishing macOS/Linux assets are future work.

## Why no Docker?

The app uses native windows, the OS WebView, local game folders and the OS credential store. Docker adds display/filesystem setup and cannot replace native Windows/macOS packaging. Use the documented toolchains and CI matrix.
