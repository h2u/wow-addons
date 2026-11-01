//! Portable replacement runs before Tauri/single-instance initialization.
//! Staging is on the destination volume, so replacement uses atomic renames.
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn staging(target: &Path) -> Result<PathBuf, String> {
    let dir = target
        .parent()
        .ok_or("Application directory is unavailable.")?
        .join(format!(".wow-addons-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&dir)
        .map_err(|e| format!("Cannot write to the portable application folder: {e}"))?;
    Ok(dir)
}
fn valid_stage(dir: &Path, target: &Path) -> bool {
    dir.parent() == target.parent()
        && dir.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            n.strip_prefix(".wow-addons-update-")
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        })
}
pub fn prepare(bytes: &[u8]) -> Result<(), String> {
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = staging(&target)?;
    let result = (|| {
        fs::write(dir.join("candidate.exe"), bytes).map_err(|e| e.to_string())?;
        fs::copy(&target, dir.join("updater.exe")).map_err(|e| e.to_string())?;
        // GNU development builds use a dynamic WebView2 loader. MSVC release builds
        // normally link it statically, but preserve an adjacent loader when present.
        let loader = target
            .parent()
            .ok_or("Missing application folder.")?
            .join("WebView2Loader.dll");
        if loader.is_file() {
            fs::copy(loader, dir.join("WebView2Loader.dll")).map_err(|e| e.to_string())?;
        }
        let mut child = Command::new(dir.join("updater.exe"))
            .arg("--apply-portable-update")
            .arg(&target)
            .arg(std::process::id().to_string())
            .arg(digest(bytes))
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !dir.join("ready").is_file() {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() || Instant::now() >= deadline
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "The portable updater could not start. The application has not been changed."
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    result
}
fn retry(mut action: impl FnMut() -> std::io::Result<()>) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match action() {
            Ok(()) => return Ok(()),
            Err(e) if Instant::now() >= deadline => return Err(e.to_string()),
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}
fn replace(
    dir: &Path,
    target: &Path,
    launch: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let backup = dir.join("previous.exe");
    retry(|| fs::rename(target, &backup))?;
    let result = retry(|| fs::rename(dir.join("candidate.exe"), target)).and_then(|()| launch());
    if let Err(error) = result {
        retry(|| {
            if target.exists() {
                fs::remove_file(target)?;
            }
            Ok(())
        })?;
        retry(|| fs::rename(&backup, target))?;
        return Err(format!(
            "Update failed; the previous version was restored: {error}"
        ));
    }
    Ok(())
}
#[cfg(windows)]
fn apply(target: &Path, pid: u32, hash: &str) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    let dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Missing updater folder.")?
        .to_path_buf();
    if !valid_stage(&dir, target) {
        return Err("Invalid portable update destination.".into());
    }
    let candidate = fs::read(dir.join("candidate.exe")).map_err(|e| e.to_string())?;
    if digest(&candidate) != hash {
        return Err("The staged update has changed. Update cancelled.".into());
    }
    // Acquire a real process handle before the parent exits; PID reuse cannot race the wait.
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        return Err(format!(
            "Could not wait for the application to close: {}",
            std::io::Error::last_os_error()
        ));
    }
    let ready = fs::write(dir.join("ready"), b"").map_err(|e| e.to_string());
    let waited = if ready.is_ok() {
        unsafe { WaitForSingleObject(process, 60_000) }
    } else {
        u32::MAX
    };
    unsafe {
        CloseHandle(process);
    }
    ready?;
    if waited != WAIT_OBJECT_0 {
        return Err("The application did not close. Update cancelled.".into());
    }
    let result = replace(&dir, target, || {
        let mut child = Command::new(target)
            .arg("--portable-update-ready")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(90);
        while !dir.join("started").is_file() {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("The new version exited during startup.".into());
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("The new version could not finish startup.".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    });
    if result.is_err() {
        let _ = Command::new(target).spawn();
    }
    if result.is_ok() {
        let _ = fs::remove_file(dir.join("previous.exe"));
    }
    fs::write(dir.join("finished"), b"").map_err(|e| e.to_string())?;
    result
}
pub fn helper_mode() -> bool {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_none_or(|s| s != "--apply-portable-update") {
        return false;
    }
    #[cfg(windows)]
    {
        let result = if args.len() == 5 {
            args[3]
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or("Invalid parent process.".to_string())
                .and_then(|pid| apply(Path::new(&args[2]), pid, &args[4].to_string_lossy()))
        } else {
            Err("Invalid portable updater arguments.".into())
        };
        if let Err(error) = result {
            if cfg!(debug_assertions) && std::env::var_os("WOW_ADDONS_TEST_DATA").is_some() {
                eprintln!("Portable update test: {error}");
                return true;
            }
            rfd::MessageDialog::new()
                .set_title("WoW Addons update")
                .set_description(error)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }
    true
}
pub fn startup_complete() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 || args[1] != "--portable-update-ready" {
        return;
    }
    let dir = PathBuf::from(&args[2]);
    let Ok(target) = std::env::current_exe() else {
        return;
    };
    if !valid_stage(&dir, &target) {
        return;
    }
    let _ = fs::write(dir.join("started"), b"");
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if dir.join("finished").is_file() && fs::remove_dir_all(&dir).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&root).unwrap();
        let target = root.join("app.exe");
        fs::write(&target, b"old").unwrap();
        let dir = staging(&target).unwrap();
        fs::write(dir.join("candidate.exe"), b"new").unwrap();
        (target, dir)
    }
    #[test]
    fn replaces_on_same_volume() {
        let (target, dir) = fixture();
        replace(&dir, &target, || Ok(())).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read(dir.join("previous.exe")).unwrap(), b"old");
        fs::remove_dir_all(target.parent().unwrap()).unwrap();
    }
    #[test]
    fn restores_if_new_app_fails() {
        let (target, dir) = fixture();
        assert!(replace(&dir, &target, || Err("startup failed".into())).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        fs::remove_dir_all(target.parent().unwrap()).unwrap();
    }
    #[test]
    fn rejects_unrelated_stage() {
        assert!(!valid_stage(
            Path::new("/other/.wow-addons-update-invalid"),
            Path::new("/app/app.exe")
        ));
    }
}
