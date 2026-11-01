use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::Duration;
use tauri::Emitter;
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct Updates {
    pending: Mutex<Option<Update>>,
    busy: AtomicBool,
}
struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn trusted(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Untrusted application window.".into())
    }
}
fn portable() -> bool {
    cfg!(windows)
        && std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|p| p.join(".wow-addons-installed").is_file())
            })
            .is_some_and(|installed| !installed)
}
#[tauri::command]
pub async fn check_app_update(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: tauri::State<'_, Updates>,
) -> Result<Value, String> {
    trusted(&window)?;
    if state.busy.swap(true, Ordering::AcqRel) {
        return Err("An application update is already in progress.".into());
    }
    let _busy = Busy(&state.busy);
    // Development builds never replace themselves or contact the release endpoint at startup.
    if cfg!(debug_assertions) || cfg!(target_os = "linux") {
        return Ok(
            json!({"supported":false,"message":"Application updates are available in Windows and macOS release builds."}),
        );
    }
    let mut builder = app.updater_builder().timeout(Duration::from_secs(30));
    if portable() {
        builder = builder.target("windows-x86_64-portable");
    }
    let mut update = builder
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Could not check for application updates: {e}"))?;
    if let Some(update) = &mut update {
        update.timeout = Some(Duration::from_secs(600));
    }
    let result = match &update {
        Some(update) => {
            json!({"supported":true,"available":true,"version":update.version,"notes":update.body,"portable":portable()})
        }
        None => json!({"supported":true,"available":false}),
    };
    *state
        .pending
        .lock()
        .map_err(|_| "Update state is unavailable.")? = update;
    Ok(result)
}
#[tauri::command]
pub async fn install_app_update(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: tauri::State<'_, Updates>,
) -> Result<(), String> {
    trusted(&window)?;
    if state.busy.swap(true, Ordering::AcqRel) {
        return Err("An application update is already in progress.".into());
    }
    let _busy = Busy(&state.busy);
    let update = state
        .pending
        .lock()
        .map_err(|_| "Update state is unavailable.")?
        .clone()
        .ok_or("Check for application updates first.")?;
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit(
                    "app-update:progress",
                    json!({"downloaded":downloaded,"total":total}),
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("Could not download or verify the update: {e}"))?;
    if portable() {
        #[cfg(windows)]
        tauri::async_runtime::spawn_blocking(move || crate::portable_update::prepare(&bytes))
            .await
            .map_err(|e| e.to_string())??;
        app.exit(0);
    } else {
        update
            .install(bytes)
            .map_err(|e| format!("Could not install the update: {e}"))?;
        app.restart();
    }
    Ok(())
}
