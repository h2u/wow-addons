#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use wow_addons_core::backend::Service;
#[tauri::command]
async fn inventory(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Mutex<Service>>>,
    method: String,
    args: Vec<Value>,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("Untrusted application window.".into());
    }
    let service = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        service
            .lock()
            .map_err(|_| "Application service is unavailable.".to_string())?
            .dispatch(&method, &args)
    })
    .await
    .map_err(|e| format!("Application task failed: {e}"))?
}
#[cfg(all(windows, any(debug_assertions, feature = "test-driver")))]
fn test_browser_args() -> Option<String> {
    std::env::var_os("WOW_ADDONS_TEST_DATA")?;
    let port: u16 = std::env::var("WOW_ADDONS_TEST_DEBUG_PORT")
        .ok()?
        .parse()
        .ok()?;
    let mut args = format!(
        "--remote-debugging-address=127.0.0.1 --remote-debugging-port={port} --no-first-run",
    );
    if std::env::var_os("CI").is_some() {
        args.push_str(" --disable-gpu");
    }
    Some(args)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![inventory])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let dir = directories::BaseDirs::new()
                .ok_or("Application data directory is unavailable.")?
                .config_dir()
                .join("wow-addons");
            #[cfg(any(debug_assertions, feature = "test-driver"))]
            let dir = std::env::var_os("WOW_ADDONS_TEST_DATA")
                .map(std::path::PathBuf::from)
                .unwrap_or(dir);
            let webview_dir = dir.join("webview");
            let service = Service::new(
                dir,
                Arc::new(move |stage| {
                    let _ = app_handle.emit("inventory:progress", stage);
                }),
            )
            .map_err(std::io::Error::other)?;
            app.manage(Arc::new(Mutex::new(service)));
            let builder =
                tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                    .data_directory(webview_dir)
                    .on_navigation(|url| {
                        url.scheme() == "tauri"
                            || (matches!(url.scheme(), "http" | "https")
                                && url.host_str() == Some("tauri.localhost"))
                    });
            #[cfg(all(windows, any(debug_assertions, feature = "test-driver")))]
            let builder = match test_browser_args() {
                Some(args) => builder.additional_browser_args(&args),
                None => builder,
            };
            let window = builder.build()?;
            #[cfg(any(debug_assertions, feature = "test-driver"))]
            if std::env::var_os("WOW_ADDONS_TEST_DATA").is_some() {
                window.set_title("WoW Addons · automated verification")?;
            }
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::DragDrop(event) = event {
                    let payload = match event {
                        tauri::DragDropEvent::Enter { paths, .. } => {
                            serde_json::json!({"type":"enter","paths":paths})
                        }
                        tauri::DragDropEvent::Over { .. } => serde_json::json!({"type":"over"}),
                        tauri::DragDropEvent::Drop { paths, .. } => {
                            serde_json::json!({"type":"drop","paths":paths})
                        }
                        tauri::DragDropEvent::Leave => serde_json::json!({"type":"leave"}),
                        _ => return,
                    };
                    let _ = app_handle.emit("inventory:drop", payload);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Could not start WoW Addons");
}
