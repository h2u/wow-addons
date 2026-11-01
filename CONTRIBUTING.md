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

### Accumulating changes

Merge ordinary PRs into main. After checks succeed, CI creates or updates one managed draft in Releases. All merged PRs since the previous stable tag appear in its notes; only application/packaging changes contribute to the version bump. The highest label wins: chore + minor gives a minor, and minor + major gives a major. Before the first stable release, the initial version is 0.1.0.

Documentation-only changes do not create a release. If application changes are already pending, subsequent documentation merges update the same draft notes. An outdated main run cannot overwrite a newer draft. CI never creates a release PR, pushes a version commit, publishes a draft automatically, or moves an existing tag.

### Publish

Open **Releases**, edit the managed draft and click **Publish release** yourself. Keep its generated version/tag. The draft targets the latest successfully checked main commit included by preparation; GitHub creates the tag on publication. The published event starts CI, checks the tagged source and builds Windows and macOS installers. New merges are collected for the next draft, not added to the published tag.

The build stamps npm, Cargo and Tauri versions from the release tag in its temporary checkout. Source manifests retain the development baseline; installed app versions and asset filenames use the published SemVer. No direct push to protected main is needed.

The release is public before the builds finish. Until both platforms succeed, assets can be absent or partially uploaded. Keep immutable releases disabled for this publish-first process. Publishing manually triggers the workflow even though the draft was prepared with GITHUB_TOKEN; no PAT or permission to create release PRs is needed.

### Manual preparation and failed builds

**Actions → CI → Run workflow → mode: prepare**, on main, refreshes the pending draft without publishing it. **mode: check** forces validation only. For failed release builds, rerun failed jobs, or use **mode: retry → tag: vX.Y.Z**, on main, to rebuild the same published tag. Retry never updates the tag to newer code. Source fixes require another PR and release. Assets are replaced by name on retry.

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

The **Release label** check rejects missing or conflicting release labels and titles that disagree with the label. Merge with **Squash and merge**, preserving the validated PR title. Draft preparation reads the release labels of merged PRs; maintenance application changes cause patch releases. Several PRs in one release use the highest requested bump.

Maintainers: in **Settings → Rules → Rulesets**, protect main, require a pull request, enable required status checks and select **Release label**, **CI result** and all three CI platform checks. The core PR/CI ruleset must have no bypasses; only the separate review ruleset allows @h2u to bypass reviews through a PR. In repository merge settings, allow only squash merging and use the PR title as the default squash commit title. The workflow alone does not prevent merging until the required check is enabled. Version override footers in PR bodies are rejected; express breaking changes through the major label and title.

## CI change detection and branch rules

One CI workflow handles PR validation, merges to main and manual releases. PRs never create releases. Check changes compares the full PR diff against its merge base, or all commits in a push; renames and deletions are included.

| Changed files                                                                                                              | Three-platform checks | Release preparation on main |
| -------------------------------------------------------------------------------------------------------------------------- | --------------------- | --------------------------- |
| Rust/frontend code, assets, capabilities, Tauri config, dependencies, packaging scripts                                    | Run                   | Run after checks pass       |
| Tests, CI workflows/actions, lint/format configuration, release automation configuration                                   | Run                   | Skip                        |
| README, CONTRIBUTING, CHANGELOG, docs/, LICENSE, .gitignore, .gitattributes, .editorconfig, CODEOWNERS, issue/PR templates | Skip                  | Skip                        |
| Unknown new paths                                                                                                          | Run                   | Run (conservative default)  |

Release preparation updates one draft after merges. Binaries are built only when you publish that draft or retry a published tag. The lightweight PR policy workflow still runs for documentation PRs and label changes.

Do not add workflow-level path filters: required jobs must report a result. The platform jobs report skipped for documentation-only changes. **CI result** fails if change detection fails, a required platform fails, or checks are unexpectedly skipped.

Import both **.github/rulesets/main.json** and **.github/rulesets/reviews.json** in Settings → Rules → Rulesets, replacing the old combined ruleset. The first requires a PR, squash merge, resolved conversations, current main and all CI checks, with no bypasses. The second requires one code-owner approval but permits @h2u (GitHub user ID 5619582) to bypass that review requirement only when merging a PR. The owner still cannot bypass CI or push directly to main. This exception lets the owner merge their own PR and can also be used by the owner on a contributor PR. CODEOWNERS remains @h2u for all files.

### Cached compilation and current main

npm download caching was already enabled. Rust caching now reuses Cargo downloads and compiled dependencies across compatible runs; the action includes the runner platform, toolchain and dependency hashes in its keys. Universal macOS packaging uses a separate cache. PR caches are scoped by GitHub to the PR merge ref; main does not trust or restore a PR-only cache. Main caches can seed later PR checks.

A cache hit never skips tests or lint. Main and release checks still run against their checked-out source. The strict required-status-check policy requires a PR to be current with main before merging; if another PR lands first, update the branch and rerun CI. Documentation-only changes continue to skip platform jobs through change detection. Do not use a cached green status as evidence that another tree was tested.
