#[path = "archive.rs"]
pub mod archive;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Addon {
    folder: String,
    name: String,
    version: String,
    interface: String,
    curse_project_id: Option<u64>,
    wago_id: Option<String>,
    dependencies: Vec<String>,
    toc: Option<String>,
    icon_path: Option<String>,
    compatibility: String,
}
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Installation {
    root: String,
    addons_path: String,
    flavor: String,
    name: String,
}
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Scan {
    installation: Installation,
    addons: Vec<Addon>,
    warnings: Vec<String>,
    scanned_at: u64,
}

fn flavor(root: &Path) -> String {
    match root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
        .as_str()
    {
        "_forever_" => "Forever",
        "_retail_" => "Retail",
        "_classic_era_" => "Classic Era",
        "_classic_" => "Classic",
        "_classic_anniversary_" => "Classic Anniversary",
        "_ptr_" | "_xptr_" => "Retail PTR",
        "_beta_" => "Retail Beta",
        "_classic_ptr_" => "Classic PTR",
        "_classic_beta_" => "Classic Beta",
        _ => "Custom",
    }
    .into()
}
fn installation(root: &Path) -> Result<Installation, String> {
    let root = fs::canonicalize(root).map_err(|e| format!("Cannot access installation: {e}"))?;
    let addons = root.join("Interface").join("AddOns");
    if !addons.is_dir() {
        return Err("No Interface/AddOns folder found. Select a WoW client folder, its executable, or the AddOns folder.".into());
    }
    let f = flavor(&root);
    Ok(Installation {
        root: root.to_string_lossy().into(),
        addons_path: addons.to_string_lossy().into(),
        name: if f == "Custom" {
            root.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into()
        } else {
            f.clone()
        },
        flavor: f,
    })
}
fn discover(input: &Path) -> Result<Vec<Installation>, String> {
    let mut root =
        fs::canonicalize(input).map_err(|e| format!("Cannot access selected path: {e}"))?;
    if root.is_file()
        || root
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
    {
        root = root.parent().ok_or("Selected path has no parent")?.into();
    }
    if root
        .file_name()
        .is_some_and(|s| s.eq_ignore_ascii_case("AddOns"))
    {
        root = root
            .parent()
            .and_then(Path::parent)
            .ok_or("Invalid AddOns path")?
            .into();
    } else if root
        .file_name()
        .is_some_and(|s| s.eq_ignore_ascii_case("Interface"))
    {
        root = root.parent().ok_or("Invalid Interface path")?.into();
    }
    if let Ok(i) = installation(&root) {
        return Ok(vec![i]);
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            if let Ok(i) = installation(&entry.path()) {
                found.push(i);
            }
        }
    }
    found.sort_by(|a, b| a.root.cmp(&b.root));
    if found.is_empty() {
        return Err(
            "No WoW clients found here. Choose a client folder containing Interface/AddOns.".into(),
        );
    }
    Ok(found)
}
// Database addons such as QuestieDB embed megabytes of data in TOC metadata.
// Match the archive's per-file limit and bound actual reads as well as sizes.
const TOC_LIMIT: u64 = 64 * 1024 * 1024;
fn read_toc(reader: impl Read) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    reader.take(TOC_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > TOC_LIMIT {
        return Err(std::io::Error::other("TOC exceeds 64 MB"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
fn metadata(text: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        if let Some(rest) = line.trim().strip_prefix("##") {
            if let Some((key, value)) = rest.split_once(':') {
                result.insert(key.trim().to_ascii_lowercase(), value.trim().into());
            }
        }
    }
    result
}
fn clean_title(text: &str) -> String {
    let mut text = text.to_string();
    for (start, end) in [("|T", "|t"), ("|A", "|a")] {
        while let Some(a) = text.find(start) {
            if let Some(b) = text[a + 2..].find(end) {
                text.replace_range(a..a + 2 + b + 2, "");
            } else {
                break;
            }
        }
    }
    while let Some(a) = text.find("|c") {
        if text
            .as_bytes()
            .get(a + 2..a + 10)
            .is_some_and(|s| s.iter().all(u8::is_ascii_hexdigit))
        {
            text.replace_range(a..a + 10, "");
        } else {
            break;
        }
    }
    text.replace("|r", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn toc_score(name: &str, folder: &str, f: &str) -> u8 {
    let n = name.to_ascii_lowercase();
    let preferred: &[&str] = match f {
        "Forever" => &["_forever.toc", "_camelot.toc"],
        "Retail" | "Retail PTR" | "Retail Beta" => &["_mainline.toc", "_retail.toc"],
        "Classic Era" => &["_vanilla.toc", "_classic.toc"],
        _ => &[],
    };
    for (index, suffix) in preferred.iter().enumerate() {
        if n.ends_with(suffix) {
            return index as u8;
        }
    }
    if n == format!("{}.toc", folder.to_ascii_lowercase()) {
        return 10;
    }
    20
}
fn local_icon(meta: &BTreeMap<String, String>, folder: &Path, addons: &Path) -> Option<String> {
    let raw = meta.get("icontexture")?;
    let normalized = raw.replace('\\', "/");
    let rel = normalized
        .strip_prefix("Interface/AddOns/")
        .or_else(|| normalized.strip_prefix("interface/addons/"));
    let candidate = if let Some(rel) = rel {
        addons.join(rel)
    } else {
        folder.join(normalized)
    };
    let candidates = if candidate.extension().is_some() {
        vec![candidate]
    } else {
        ["png", "jpg", "jpeg", "webp", "gif"]
            .iter()
            .map(|ext| candidate.with_extension(ext))
            .collect()
    };
    candidates.into_iter().find_map(|p| {
        let ext = p.extension()?.to_str()?.to_ascii_lowercase();
        if !["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()) {
            return None;
        }
        let canonical = fs::canonicalize(p).ok()?;
        let base = fs::canonicalize(addons).ok()?;
        if !canonical.starts_with(base) || fs::metadata(&canonical).ok()?.len() > 1024 * 1024 {
            return None;
        }
        Some(canonical.to_string_lossy().into())
    })
}
fn scan(root: &Path) -> Result<Scan, String> {
    let install = installation(root)?;
    let addons_path = Path::new(&install.addons_path);
    let mut addons = Vec::new();
    let mut warnings = Vec::new();
    for item in fs::read_dir(addons_path).map_err(|e| format!("Cannot read AddOns: {e}"))? {
        let item = item.map_err(|e| format!("Cannot enumerate AddOns: {e}"))?;
        let kind = item.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            warnings.push(format!(
                "Skipped symbolic link: {}",
                item.file_name().to_string_lossy()
            ));
            continue;
        }
        if !kind.is_dir() {
            continue;
        }
        let folder = item.file_name().to_string_lossy().to_string();
        let mut tocs: Vec<PathBuf> = match fs::read_dir(item.path()) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("toc")))
                .collect(),
            Err(e) => {
                warnings.push(format!("{folder}: {e}"));
                Vec::new()
            }
        };
        tocs.sort_by_key(|p| {
            (
                toc_score(
                    &p.file_name().unwrap_or_default().to_string_lossy(),
                    &folder,
                    &install.flavor,
                ),
                p.clone(),
            )
        });
        let toc = tocs.first();
        let meta = if let Some(toc) = toc {
            match fs::File::open(toc).and_then(read_toc) {
                Ok(text) => metadata(&text),
                Err(e) => {
                    warnings.push(format!("{folder}: {e}"));
                    BTreeMap::new()
                }
            }
        } else {
            warnings.push(format!("{folder}: no readable TOC file"));
            BTreeMap::new()
        };
        let interface = meta.get("interface").cloned().unwrap_or_default();
        let compatibility = if toc.is_none() || interface.is_empty() {
            "Unknown"
        } else if install.flavor == "Forever" {
            if interface.split(',').any(|s| s.trim() == "16001") {
                "Forever"
            } else {
                "Review compatibility"
            }
        } else {
            "Declared in TOC"
        };
        let name = meta
            .get("title")
            .map(|s| clean_title(s))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| folder.clone());
        addons.push(Addon {
            name,
            version: meta.get("version").cloned().unwrap_or_default(),
            curse_project_id: meta.get("x-curse-project-id").and_then(|s| s.parse().ok()),
            wago_id: meta.get("x-wago-id").cloned(),
            dependencies: meta
                .get("dependencies")
                .or_else(|| meta.get("requireddeps"))
                .map(|s| {
                    s.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            toc: toc.map(|p| p.file_name().unwrap_or_default().to_string_lossy().into()),
            icon_path: local_icon(&meta, &item.path(), addons_path),
            compatibility: compatibility.into(),
            interface,
            folder,
        });
    }
    addons.sort_by_key(|a| a.name.to_lowercase());
    Ok(Scan {
        installation: install,
        addons,
        warnings,
        scanned_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}
pub fn run(command: &str, input: &str, extra: &[String]) -> Result<serde_json::Value, String> {
    let mut args = vec![
        "wow-addons-core".to_string(),
        command.to_string(),
        input.to_string(),
    ];
    args.extend_from_slice(extra);
    let result = match (args.get(1).map(String::as_str), args.get(2)) {
        (Some("discover"), Some(path)) => discover(Path::new(path))
            .and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string())),
        (Some("scan"), Some(path)) => {
            scan(Path::new(path)).and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string()))
        }
        (Some("inspect-zip"), Some(root)) => args
            .get(3)
            .ok_or("ZIP path required".to_string())
            .and_then(|zip| archive::inspect(Path::new(root), Path::new(zip))),
        (Some("install-zip"), Some(root)) => match (args.get(3), args.get(4)) {
            (Some(zip), Some(backup)) => {
                archive::install(Path::new(root), Path::new(zip), Path::new(backup))
            }
            _ => Err("ZIP and backup paths required".into()),
        },
        (Some("remove"), Some(root)) => match (args.get(3), args.get(4)) {
            (Some(folders), Some(backup)) => {
                archive::remove(Path::new(root), folders, Path::new(backup))
            }
            _ => Err("Folder list and backup path required".into()),
        },
        _ => Err(
            "Usage: wow-addons-core <discover|scan|inspect-zip|install-zip|remove> <path> ..."
                .into(),
        ),
    };
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, process};
    fn fixture() -> PathBuf {
        let p = env::temp_dir().join(format!(
            "wow-addons-test-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(p.join("_forever_/Interface/AddOns/Test")).unwrap();
        p
    }
    #[test]
    fn bounds_actual_toc_reads() {
        let oversized = std::io::repeat(b'x').take(TOC_LIMIT + 100);
        assert!(read_toc(oversized)
            .unwrap_err()
            .to_string()
            .contains("64 MB"));
    }
    #[test]
    fn cleans_wow_markup_and_bom() {
        let m = metadata("\u{feff}## Title: |Ticon:16|t |cff00ff00Test|r\n## Version: 1.2\n## X-Curse-Project-ID: 123");
        assert_eq!(clean_title(&m["title"]), "Test");
        assert_eq!(m["x-curse-project-id"], "123");
    }
    #[test]
    fn discovers_clients_and_prefers_forever_toc() {
        let p = fixture();
        let root = p.join("_forever_");
        let a = root.join("Interface/AddOns/Test");
        fs::write(
            a.join("Test.toc"),
            "## Title: Retail\n## Interface: 120105\n## Version: wrong",
        )
        .unwrap();
        fs::write(
            a.join("Test_Camelot.toc"),
            "## Title: Camelot\n## Interface: 16001\n## Version: 1",
        )
        .unwrap();
        fs::write(
            a.join("Test_Forever.toc"),
            "## Title: Forever\n## Interface: 16001\n## Version: 2\n## X-Curse-Project-ID: 123",
        )
        .unwrap();
        fs::write(root.join("Wow.exe"), "").unwrap();
        for input in [
            &p,
            &root,
            &root.join("Wow.exe"),
            &root.join("Interface/AddOns"),
        ] {
            assert_eq!(discover(input).unwrap().len(), 1);
        }
        let result = scan(&root).unwrap();
        assert_eq!(result.addons[0].name, "Forever");
        assert_eq!(result.addons[0].version, "2");
        assert_eq!(result.addons[0].curse_project_id, Some(123));
        assert_eq!(result.addons[0].compatibility, "Forever");
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn reports_mismatch_and_keeps_unknown_folders() {
        let p = fixture();
        let root = p.join("_forever_");
        fs::write(
            root.join("Interface/AddOns/Test/Test.toc"),
            "## Interface: 120105",
        )
        .unwrap();
        fs::create_dir(root.join("Interface/AddOns/Unknown")).unwrap();
        let result = scan(&root).unwrap();
        assert_eq!(result.addons.len(), 2);
        assert!(result
            .addons
            .iter()
            .any(|a| a.compatibility == "Review compatibility"));
        assert!(!result.warnings.is_empty());
        assert!(discover(&root.join("missing")).is_err());
        fs::remove_dir_all(p).unwrap();
    }
}
