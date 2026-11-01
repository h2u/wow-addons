#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
struct Lifecycle {
    close_to_tray: AtomicBool,
    tray_available: bool,
}
fn show_main(app: &tauri::AppHandle) {
    // Native tray and single-instance callbacks can run inside a platform window procedure.
    // Queue window work to avoid blocking that procedure on the event loop.
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    });
}
#[tauri::command]
fn desktop_settings(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("Untrusted application window.".into());
    }
    Ok(serde_json::json!({
        "autostart": app.autolaunch().is_enabled().map_err(|e| e.to_string())?,
        "trayAvailable": app.state::<Lifecycle>().tray_available,
    }))
}
#[tauri::command]
fn set_autostart(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Untrusted application window.".into());
    }
    #[cfg(windows)]
    {
        // Register only for this user and quote paths containing spaces (including portable copies).
        use windows_registry::CURRENT_USER;
        let name = &app.package_info().name;
        let key = CURRENT_USER
            .create(r"Software\Microsoft\Windows\CurrentVersion\Run")
            .map_err(|e| e.to_string())?;
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            key.set_string(name, format!("\"{}\"", exe.display()))
                .map_err(|e| e.to_string())?;
            let approved = CURRENT_USER
                .create(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run")
                .map_err(|e| e.to_string())?;
            approved
                .set_bytes(
                    name,
                    windows_registry::Type::Bytes,
                    &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                )
                .map_err(|e| e.to_string())?;
        } else if key.get_string(name).is_ok() {
            key.remove_value(name).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if enabled {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        }
        .map_err(|e| e.to_string())
    }
}
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let show = MenuItem::with_id(app, "show", "Open WoW Addons", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("WoW Addons")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
use wow_addons_core::backend::Service;
#[tauri::command]
async fn inventory(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<Service>>>,
    method: String,
    args: Vec<Value>,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("Untrusted application window.".into());
    }
    let service = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = service
            .lock()
            .map_err(|_| "Application service is unavailable.".to_string())?
            .dispatch(&method, &args)?;
        if method == "preferences" {
            if let Some(enabled) = result["closeToTray"].as_bool() {
                app.state::<Lifecycle>()
                    .close_to_tray
                    .store(enabled, Ordering::Relaxed);
            }
        }
        Ok(result)
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
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            inventory,
            desktop_settings,
            set_autostart
        ])
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
            let close_to_tray = service.state["closeToTray"].as_bool().unwrap_or(true);
            let tray_available = match setup_tray(app) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("System tray unavailable: {e}");
                    false
                }
            };
            app.manage(Lifecycle {
                close_to_tray: AtomicBool::new(close_to_tray),
                tray_available,
            });
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
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let lifecycle = app_handle.state::<Lifecycle>();
                    if lifecycle.tray_available && lifecycle.close_to_tray.load(Ordering::Relaxed) {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            if window.hide().is_ok() {
                                api.prevent_close();
                            }
                        }
                    }
                }
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
