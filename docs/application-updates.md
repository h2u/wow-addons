# Application updates

Release builds check GitHub for a newer stable application version at startup. Settings → About also provides **Check for Updates**. Downloading and installation begin only after **Update and Restart**. Application updates do not use the CurseForge API key. Debug builds and Linux source builds do not self-update.

Windows NSIS installations use the signed Tauri installer with passive progress UI. The installer records `.wow-addons-installed` in its installation directory so a portable executable never starts an installer. Portable builds download a separately signed executable. A temporary copy of the current app runs as the updater, waits on the parent process handle, swaps files on the same volume, and starts the new app at the original path. Settings remain in the existing user configuration directory. The executable name/path and login registration remain valid.

The portable updater keeps the old executable until the new app initializes its service and main window. Failed replacement or startup triggers restoration and restart of the previous executable. A read-only application directory fails before the app closes. Running the app from a folder writable by the current user is required for portable updates. A locked executable is retried for 15 seconds; startup has a 90-second deadline. Startup completion confirms initialization, not every later application operation.

## Signing setup (once)

The public verification key is committed in `tauri.conf.json`. Its matching private key was generated locally in the ignored `.tools/updater.key` file. The matching key has also been configured as the repository Actions secret `TAURI_SIGNING_PRIVATE_KEY` for `h2u/wow-addons`. Keep a separate secure backup of the local key. The generated key has an empty password; set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` only if you later use a password-protected key. Never commit the private key or publish the ignored key-generation log.

Losing the key prevents future updates for existing installations. Changing the committed public key without a transition breaks their update chain. Updater artifact signatures are separate from Windows Authenticode/macOS distribution signing.

## Publishing

Use the existing release workflow and a stable `vX.Y.Z` tag. Both release jobs require the signing secret. Tauri signs the NSIS installer and macOS updater archive; the export script signs the portable executable with its application version. Both platforms upload their artifacts and `.sig` files. Only after both jobs succeed does the final job publish `latest.json`, mapping installed Windows, portable Windows, and both macOS architectures to the appropriate signed files. The client requires signatures bound to the announced version.

No separate update server is required. GitHub hosts the manifest and downloads. Drafts and prereleases are not the stable update channel. Until a release containing the updater has been installed manually, older apps cannot update themselves.

## Verification

`npm test` covers manifest selection and update-check UI behavior. Rust unit tests cover replacement and rollback; Windows binary tests run these on Windows too. `node scripts/test-portable-update.cjs` runs native process waiting, paths with spaces, startup acknowledgement, and rollback checks on isolated copies (Windows, debug binary, and rustc required). CI runs this after the native UI suite. File-lock retries still need coverage in release acceptance testing. Before publishing, test a real signed upgrade between two release versions for both NSIS and portable, plus macOS on a Mac. A production release has not been published by this implementation task.
