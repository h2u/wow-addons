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

| Path                                              | Responsibility                                                                   |
| ------------------------------------------------- | -------------------------------------------------------------------------------- |
| `src/desktop_main.rs`                             | Tauri window, event bridge and background command dispatch                       |
| `src/backend.rs`                                  | Serialized application service, profiles, settings, cache and package operations |
| `src/inventory.rs`                                | Client discovery and TOC scanning                                                |
| `src/archive.rs`                                  | ZIP paths, compatibility, backups, staging and rollback                          |
| `src/provider.rs`                                 | CurseForge API, releases, downloads, hashes and update assessment                |
| `src/grouping.rs`                                 | Package ownership and manifests                                                  |
| `src/images.rs`                                   | Local image decoding, including TGA and BLP2 DXT                                 |
| `src/migration.rs`                                | One-time native Windows credential migration                                     |
| `desktop/bridge.js`                               | Narrow Tauri command API for the renderer                                        |
| `desktop/renderer.js`, `index.html`, `styles.css` | UI                                                                               |
| `desktop/grouping.js`, `bundles.json`             | Display grouping and known bundle layouts                                        |
| `capabilities/`, `tauri*.conf.json`               | Native permissions and packaging                                                 |
| `tests/`, inline Rust tests                       | Frontend and backend verification                                                |

All operations use Rust directly; there is no sidecar executable or Electron runtime. The optional `wow-addons-core` CLI is a debugging tool that calls the same library.

The frontend cannot fetch arbitrary URLs or use a filesystem plugin. The native service validates commands, URL hosts, paths and installer plans. Navigation is restricted to the local UI. Preserve this boundary when adding features.

## Formatting and linting

Use **rustfmt** for Rust layout and **Clippy** for Rust diagnostics. Prettier formats JavaScript, HTML, CSS, JSON, YAML and Markdown; ESLint checks JavaScript correctness. These tools are not bundled with the application.

```sh
rustup component add rustfmt clippy
npm run format
npm run format:rust
```

Before opening a PR, run:

```sh
npm run format:check
npm run lint
npm run check
npm test
cargo fmt --check
npm run lint:rust
cargo test --locked --lib
```

`npm run lint:fix` applies safe ESLint fixes; review the diff afterward. `.editorconfig`, `.prettierrc.json`, `eslint.config.cjs` and `rustfmt.toml` define the shared style. Rust uses four-space indentation; frontend code uses two spaces. Use multiline object fields in long `json!` macros, which rustfmt cannot lay out automatically.

Editor integrations: use rust-analyzer for Rust, and Prettier and ESLint for frontend code. Enable format on save locally. CI rejects formatting differences and lint warnings. Generated outputs, dependencies and vendored SVGs are excluded from formatting.

## Tests

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

Build on the target OS and architecture. Outputs are in `target/release/` and `target/release/bundle/`. Windows NSIS installs WebView2 when needed; a portable EXE requires it already installed. The release workflow builds a universal macOS DMG. Linux can be built from source; complete game-management testing on macOS/Linux is still pending.

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

The app starts at **0.1.0**. Direct pushes to `main` are not required for any release step.

### First release

After the setup PR is merged, open **Actions → Release → Run workflow**, select branch **main**, and use **mode: initial**. The workflow reads the committed version, creates a `v0.1.0` tag and draft, and builds Windows x64 and macOS universal assets. It publishes only after both platforms succeed. It does not commit or push a version change to the protected branch.

Before the first published stable release, pushes to `main` skip Release Please with an informational notice.

### Subsequent releases

Release Please collects Conventional Commits after the published baseline and opens a release PR with the next SemVer, changelog and synchronized npm/Cargo/Tauri versions. Merge that PR to trigger both platform builds. Normal feature PRs do not edit versions manually.

Enable **Settings → Actions → General → Workflow permissions → Allow GitHub Actions to create and approve pull requests** before using release PRs. The first manual release does not need this setting. No CurseForge secret or release PAT is required.

If protected branches require CI on bot-created PRs, manually run CI on the release PR branch, or configure a scoped GitHub App token whose PR events trigger CI.

### Failed builds

A failed build leaves a draft. Use **Run workflow → mode: retry → tag: vX.Y.Z** to retry the tagged source. A source correction needs a new version through a PR; do not move an existing tag.

Windows assets contain a portable EXE, an NSIS installer and SHA256 checksums. macOS assets contain a universal DMG for Intel and Apple Silicon and a separate checksum. The Mac package is ad-hoc signed, without Apple notarization; full game-management testing on a Mac is still needed.

Linux remains a compile/test target and an optional source build. WoW has no official native Linux client; Linux release assets are not published.

### Windows UI runner

Native UI tests currently use `windows-2022` because of the documented [WebView2 automation regression on the Windows 2025 image](https://github.com/actions/runner-images/issues/14738). The setup action checks/installs WebView2. Tests pass debug flags through the SDK only in a debug/test build, capture startup output, and upload `ui-startup.json` on failure. Production builds do not enable the test hooks.

## Why no Docker?

The app uses native windows, the OS WebView, local game folders and the OS credential store. Docker adds display/filesystem setup and cannot replace native Windows/macOS packaging. Use the documented toolchains and CI matrix.

### Required release label

Every PR requires exactly one of these labels. Other descriptive labels are allowed.

| Label | PR title                                   | Version change from 0.1.0 |
| ----- | ------------------------------------------ | ------------------------- |
| major | `feat!: description`                       | 1.0.0                     |
| minor | `feat: description`                        | 0.2.0                     |
| chore | `chore: description` or `fix: description` | 0.1.1                     |

The **Release label** check rejects missing or conflicting release labels and titles that disagree with the label. Merge with **Squash and merge**, preserving the validated PR title. Release Please uses that conventional squash commit; maintenance changes are included in release notes and cause patch releases. Several PRs in one release use the highest requested bump. Generated release PRs receive the chore label and keep their chore release title.

Maintainers: in **Settings → Rules → Rulesets**, protect main, require a pull request, enable required status checks and select **Release label** plus all three CI platform checks. Do not allow bypasses. In repository merge settings, allow only squash merging and use the PR title as the default squash commit title. The workflow alone does not prevent merging until the required check is enabled. For GitHub Actions bot PRs, run CI manually on the release branch and remove/re-add the chore label yourself to trigger PR policy. Alternatively use a GitHub App token whose events trigger workflows. Version override footers in PR bodies are rejected; express breaking changes through the major label and title.
