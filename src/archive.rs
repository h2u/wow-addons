use super::{flavor, metadata, read_toc, toc_score, TOC_LIMIT};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::ZipArchive;

const TOTAL_LIMIT: u64 = 512 * 1024 * 1024;
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub folder: String,
    pub version: String,
    pub interface: String,
    pub exists: bool,
}
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub packages: Vec<Package>,
    pub skipped: Vec<Package>,
    pub flavor: String,
    pub uncompressed_bytes: u64,
}
struct Entry {
    index: usize,
    parts: Vec<String>,
    is_dir: bool,
    size: u64,
}
fn safe_parts(name: &str) -> Result<Vec<String>, String> {
    if name.starts_with('/') || name.contains('\\') || name.contains(':') {
        return Err("Archive contains an unsafe path".into());
    }
    let parts: Vec<String> = name
        .trim_end_matches('/')
        .split('/')
        .map(str::to_string)
        .collect();
    if parts.iter().any(|s| {
        s.is_empty()
            || s == "."
            || s == ".."
            || s.ends_with('.')
            || s.ends_with(' ')
            || s.chars().any(|c| c.is_control())
    }) {
        return Err("Archive contains an unsafe path".into());
    }
    for part in &parts {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err("Archive contains a reserved filename".into());
        }
    }
    Ok(parts)
}
fn interface_family(root: &Path, f: &str) -> Option<u32> {
    match f {
        "Forever" => return Some(160),
        "Classic Era" => return Some(115),
        "Retail" | "Retail PTR" | "Retail Beta" => return Some(0),
        _ => {}
    }
    let product = if f.starts_with("Classic") {
        "wow_classic"
    } else {
        return None;
    };
    let text = fs::read_to_string(root.parent()?.join(".build.info")).ok()?;
    let mut lines = text.lines();
    let headers: Vec<_> = lines.next()?.split('|').collect();
    let p = headers.iter().position(|h| h.starts_with("Product!"))?;
    let v = headers.iter().position(|h| h.starts_with("Version!"))?;
    for line in lines {
        let fields: Vec<_> = line.split('|').collect();
        if fields.get(p) == Some(&product) {
            let version: Vec<_> = fields.get(v)?.split('.').collect();
            return Some(
                version.first()?.parse::<u32>().ok()? * 100
                    + version.get(1)?.parse::<u32>().ok()?,
            );
        }
    }
    None
}
fn compatible(interface: &str, family: u32) -> bool {
    interface
        .split(',')
        .filter_map(|v| v.trim().parse::<u32>().ok())
        .any(|v| {
            if family == 0 {
                v >= 100000
            } else {
                v / 100 == family
            }
        })
}
fn analyze(
    root: &Path,
    archive: &mut ZipArchive<fs::File>,
) -> Result<(Plan, Vec<Entry>, Vec<String>), String> {
    super::installation(root)?;
    if archive.len() > 20000 {
        return Err("Archive has too many files".into());
    }
    let mut entries = Vec::new();
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    let mut tocs = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("Symbolic links are not supported in addon ZIP files".into());
        }
        let normalized_name = file.name().replace('\\', "/");
        let parts = safe_parts(&normalized_name)?;
        if !paths.insert(parts.join("/").to_lowercase()) {
            return Err("Archive contains duplicate or case-conflicting paths".into());
        }
        total = total
            .checked_add(file.size())
            .ok_or("Archive is too large")?;
        if total > TOTAL_LIMIT || file.size() > 64 * 1024 * 1024 {
            return Err("Archive exceeds extraction limits".into());
        }
        if !file.is_dir()
            && parts
                .last()
                .is_some_and(|n| n.to_lowercase().ends_with(".toc"))
        {
            if file.size() > TOC_LIMIT {
                return Err("TOC file is too large".into());
            }
            let text = read_toc(&mut file).map_err(|e| e.to_string())?;
            tocs.push((parts.clone(), metadata(&text)));
        }
        entries.push(Entry {
            index: i,
            parts,
            is_dir: normalized_name.ends_with('/'),
            size: file.size(),
        });
    }
    let depth = tocs
        .iter()
        .map(|(p, _)| p.len() - 1)
        .min()
        .ok_or("ZIP contains no addon TOC files")?;
    if depth == 0 || depth > 4 {
        return Err("ZIP must contain addon folders, each with a TOC file".into());
    }
    let first = &tocs.iter().find(|(p, _)| p.len() - 1 == depth).unwrap().0;
    let prefix = first[..depth - 1].to_vec();
    let f = flavor(root);
    let family = interface_family(root, &f).ok_or(
        "Cannot determine this client's interface version. Check the client's .build.info file.",
    )?;
    let mut packages: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for (parts, meta) in &tocs {
        if parts.len() - 1 == depth {
            if parts[..depth - 1] != prefix {
                return Err("ZIP has an ambiguous addon folder layout".into());
            }
            packages
                .entry(parts[depth - 1].clone())
                .or_default()
                .push((parts.last().unwrap().clone(), meta));
        }
    }
    let mut plan = Vec::new();
    let mut rejected = Vec::new();
    let mut required = BTreeSet::new();
    for (folder, mut variants) in packages {
        variants.sort_by_key(|(name, _)| toc_score(name, &folder, &f));
        let selected = variants.iter().find(|(_, meta)| {
            compatible(
                meta.get("interface").map(String::as_str).unwrap_or(""),
                family,
            )
        });
        if selected.is_none() {
            let meta = variants[0].1;
            let deps = meta
                .get("dependencies")
                .or_else(|| meta.get("requireddeps"))
                .map(|v| {
                    v.split(',')
                        .map(str::trim)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let group = meta.get("group").cloned();
            let interfaces = variants
                .iter()
                .filter_map(|(_, m)| m.get("interface"))
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            let target = root.join("Interface/AddOns").join(&folder);
            rejected.push((
                Package {
                    folder,
                    version: meta.get("version").cloned().unwrap_or_default(),
                    interface: interfaces,
                    exists: target.exists(),
                },
                deps,
                group,
            ));
            continue;
        }
        let selected = selected.unwrap();
        if let Some(deps) = selected
            .1
            .get("dependencies")
            .or_else(|| selected.1.get("requireddeps"))
        {
            for dep in deps.split(',').map(str::trim) {
                required.insert(dep.to_string());
            }
        }
        let target = root.join("Interface/AddOns").join(&folder);
        if let Ok(m) = fs::symlink_metadata(&target) {
            if m.file_type().is_symlink() || !m.is_dir() {
                return Err(format!("{folder}: target is not a normal addon directory"));
            }
        }
        plan.push(Package {
            folder,
            version: selected.1.get("version").cloned().unwrap_or_default(),
            interface: selected.1.get("interface").cloned().unwrap_or_default(),
            exists: target.exists(),
        });
    }
    let supported: BTreeSet<_> = plan.iter().map(|p| p.folder.clone()).collect();
    let mut skipped = Vec::new();
    for (package, deps, group) in rejected {
        let bundled = deps.iter().any(|d| supported.contains(d))
            || group.as_ref().is_some_and(|g| supported.contains(g));
        if bundled && !required.contains(&package.folder) {
            skipped.push(package);
        } else {
            let expected = if family == 0 {
                "Retail (100000+)".to_string()
            } else {
                format!("{family}xx")
            };
            return Err(format!(
                "{} is not compatible with {f}. Declared interfaces: [{}]; \
                 expected {expected}. Choose a ZIP built for this client.",
                package.folder, package.interface,
            ));
        }
    }
    Ok((
        Plan {
            packages: plan,
            skipped,
            flavor: f,
            uncompressed_bytes: total,
        },
        entries,
        prefix,
    ))
}
type OpenedArchive = (ZipArchive<fs::File>, Plan, Vec<Entry>, Vec<String>);

fn open(root: &Path, zip: &Path) -> Result<OpenedArchive, String> {
    if fs::metadata(zip).map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024 {
        return Err("ZIP exceeds 256 MB".into());
    }
    let mut archive = ZipArchive::new(fs::File::open(zip).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Invalid ZIP: {e}"))?;
    let (plan, entries, prefix) = analyze(root, &mut archive)?;
    Ok((archive, plan, entries, prefix))
}
pub fn inspect(root: &Path, zip: &Path) -> Result<serde_json::Value, String> {
    let (_, plan, _, _) = open(root, zip)?;
    serde_json::to_value(plan).map_err(|e| e.to_string())
}
fn copy_dir(source: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir(dest).map_err(|e| e.to_string())?;
    for item in fs::read_dir(source).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let kind = item.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Cannot back up an addon containing symbolic links".into());
        }
        let target = dest.join(item.file_name());
        if kind.is_dir() {
            copy_dir(&item.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(item.path(), target).map_err(|e| e.to_string())?;
        } else {
            return Err("Unsupported addon file type".into());
        }
    }
    Ok(())
}
fn stage(root: &Path) -> Result<PathBuf, String> {
    let base = fs::canonicalize(root.join("Interface")).map_err(|e| e.to_string())?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !base.starts_with(&root) {
        return Err("Interface folder points outside the selected game".into());
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = base.join(format!(".wow-addons-stage-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).map_err(|e| e.to_string())?;
    Ok(path)
}
fn remove_stage(path: &Path, root: &Path) -> Result<(), String> {
    let canonical = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let base = fs::canonicalize(root.join("Interface")).map_err(|e| e.to_string())?;
    if canonical.parent() != Some(base.as_path())
        || !canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with(".wow-addons-stage-")
    {
        return Err("Unsafe staging directory".into());
    }
    fs::remove_dir_all(canonical).map_err(|e| e.to_string())
}
fn exchange(
    root: &Path,
    stage: &Path,
    folders: &[String],
    backup: &Path,
    install: bool,
    skip: &[String],
) -> Result<(), String> {
    let addons = fs::canonicalize(root.join("Interface/AddOns")).map_err(|e| e.to_string())?;
    let game = fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !addons.starts_with(&game) {
        return Err("AddOns folder points outside the selected game".into());
    }
    let old = stage.join("old");
    fs::create_dir(&old).map_err(|e| e.to_string())?;
    fs::create_dir_all(backup).map_err(|e| e.to_string())?;
    for folder in folders {
        if safe_parts(folder)?.len() != 1 {
            return Err("Invalid addon folder".into());
        }
        let target = addons.join(folder);
        if let Ok(meta) = fs::symlink_metadata(&target) {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(format!("{folder}: unsafe target"));
            }
            copy_dir(&target, &backup.join(folder))?;
        }
    }
    fs::write(
        stage.join("transaction.json"),
        serde_json::to_vec(
            &serde_json::json!({"folders":folders,"backupPath":backup,"install":install}),
        )
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let mut moved = Vec::new();
    let mut installed = Vec::new();
    let attempt = (|| -> Result<(), String> {
        for folder in folders {
            let target = addons.join(folder);
            if target.exists() {
                fs::rename(&target, old.join(folder)).map_err(|e| e.to_string())?;
                moved.push(folder.clone());
            }
        }
        if install {
            for folder in folders.iter().filter(|f| !skip.contains(f)) {
                fs::rename(stage.join("new").join(folder), addons.join(folder))
                    .map_err(|e| e.to_string())?;
                installed.push(folder.clone());
            }
        }
        Ok(())
    })();
    if let Err(error) = attempt {
        let mut failures = Vec::new();
        for folder in installed.iter().rev() {
            if let Err(e) = fs::rename(addons.join(folder), stage.join("new").join(folder)) {
                failures.push(e.to_string());
            }
        }
        for folder in moved.iter().rev() {
            if let Err(e) = fs::rename(old.join(folder), addons.join(folder)) {
                failures.push(e.to_string());
            }
        }
        if !failures.is_empty() {
            return Err(format!(
                "{error}. Recovery files preserved at {}. Rollback: {}",
                stage.display(),
                failures.join("; ")
            ));
        }
        return Err(format!(
            "{error}. Original folders restored; backup: {}",
            backup.display()
        ));
    }
    Ok(())
}
pub fn install(root: &Path, zip: &Path, backup: &Path) -> Result<serde_json::Value, String> {
    let (mut archive, plan, entries, prefix) = open(root, zip)?;
    let folders: Vec<String> = plan.packages.iter().map(|p| p.folder.clone()).collect();
    let stage = stage(root)?;
    let new = stage.join("new");
    fs::create_dir(&new).map_err(|e| e.to_string())?;
    let extracted = (|| -> Result<(), String> {
        for entry in entries {
            if !entry.parts.starts_with(&prefix) || entry.parts.len() <= prefix.len() {
                continue;
            }
            let rel = &entry.parts[prefix.len()..];
            if !folders.contains(&rel[0]) {
                continue;
            }
            let target = rel.iter().fold(new.clone(), |p, s| p.join(s));
            if entry.is_dir {
                fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                continue;
            }
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            let mut input = archive.by_index(entry.index).map_err(|e| e.to_string())?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|e| e.to_string())?;
            let copied = std::io::copy(&mut input.by_ref().take(entry.size + 1), &mut output)
                .map_err(|e| e.to_string())?;
            if copied != entry.size {
                return Err("ZIP entry size mismatch".into());
            }
            output.flush().map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(e) = extracted {
        let _ = remove_stage(&stage, root);
        return Err(e);
    }
    let skipped: Vec<String> = plan.skipped.iter().map(|p| p.folder.clone()).collect();
    let mut changed = folders.clone();
    changed.extend(skipped.clone());
    exchange(root, &stage, &changed, backup, true, &skipped)?;
    let cleanup = remove_stage(&stage, root).err();
    Ok(serde_json::json!({
        "installed": folders,
        "skipped": skipped,
        "backupPath": backup,
        "cleanupWarning": cleanup,
    }))
}
pub fn remove(root: &Path, folder_json: &str, backup: &Path) -> Result<serde_json::Value, String> {
    super::installation(root)?;
    let folders: Vec<String> =
        serde_json::from_str(folder_json).map_err(|_| "Invalid folder list")?;
    if folders.is_empty() || folders.len() > 1000 {
        return Err("Invalid folder count".into());
    }
    let stage = stage(root)?;
    exchange(root, &stage, &folders, backup, false, &[])?;
    let cleanup = remove_stage(&stage, root).err();
    Ok(serde_json::json!({"removed":folders,"backupPath":backup,"cleanupWarning":cleanup}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};
    fn fixture() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "wow-zip-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(p.join("_forever_/Interface/AddOns")).unwrap();
        p
    }
    fn zip(path: &Path, files: &[(&str, &str)]) {
        let mut z = ZipWriter::new(fs::File::create(path).unwrap());
        for (n, s) in files {
            z.start_file(*n, SimpleFileOptions::default()).unwrap();
            z.write_all(s.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    #[test]
    fn installs_large_database_toc_and_scans_its_metadata() {
        let p = fixture();
        let root = p.join("_forever_");
        let z = p.join("database.zip");
        // QuestieDB embeds its database in TOC metadata, beyond the old 1 MB cap.
        let toc = format!(
            "## Interface: 16001\n## Title: QuestieDB\n## Version: 1.0.13\n## X-Data: {}\n## X-Curse-Project-ID: 1696795\n",
            "x".repeat(2 * 1024 * 1024)
        );
        zip(&z, &[("QuestieDB/QuestieDB_Forever.toc", &toc)]);
        let plan = inspect(&root, &z).unwrap();
        assert_eq!(plan["packages"][0]["version"], "1.0.13");
        install(&root, &z, &p.join("backup")).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("Interface/AddOns/QuestieDB/QuestieDB_Forever.toc"))
                .unwrap(),
            toc
        );
        let scan = super::super::scan(&root).unwrap();
        assert!(scan.warnings.is_empty());
        assert_eq!(scan.addons[0].name, "QuestieDB");
        assert_eq!(scan.addons[0].version, "1.0.13");
        assert_eq!(scan.addons[0].curse_project_id, Some(1696795));
        assert_eq!(scan.addons[0].compatibility, "Forever");
        assert_eq!(
            crate::provider::assess(
                &scan.addons[0].version,
                None,
                &serde_json::json!({"id": 9514215, "displayName": "v1.0.13 (Forever)"}),
                &serde_json::Value::Null,
                &serde_json::Value::Null,
            )["status"],
            "current"
        );
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn handles_windows_zip_paths_and_rejects_windows_traversal() {
        let p = fixture();
        let root = p.join("_forever_");
        let z = p.join("test.zip");
        zip(
            &z,
            &[
                ("Test\\Test.toc", "## Interface: 16001"),
                ("Test\\nested\\file.lua", "ok"),
            ],
        );
        install(&root, &z, &p.join("backup")).unwrap();
        assert!(root.join("Interface/AddOns/Test/nested/file.lua").exists());
        for bad in [
            "..\\escape.lua",
            "Test\\..\\escape.lua",
            "C:\\escape.lua",
            "\\\\server\\share\\escape.lua",
        ] {
            zip(
                &z,
                &[(bad, "bad"), ("Test\\Test.toc", "## Interface: 16001")],
            );
            assert!(inspect(&root, &z).is_err());
        }
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn installs_replaces_backs_up_and_removes() {
        let p = fixture();
        let root = p.join("_forever_");
        let z = p.join("test.zip");
        fs::create_dir(root.join("Interface/AddOns/Test")).unwrap();
        fs::write(root.join("Interface/AddOns/Test/old.lua"), "old").unwrap();
        zip(
            &z,
            &[
                ("Test/Test.toc", "## Interface: 16001\n## Version: 2"),
                ("Test/new.lua", "new"),
            ],
        );
        assert_eq!(inspect(&root, &z).unwrap()["packages"][0]["exists"], true);
        install(&root, &z, &p.join("backup")).unwrap();
        assert!(p.join("backup/Test/old.lua").exists());
        assert!(!root.join("Interface/AddOns/Test/old.lua").exists());
        remove(&root, "[\"Test\"]", &p.join("removed")).unwrap();
        assert!(!root.join("Interface/AddOns/Test").exists());
        assert!(p.join("removed/Test/new.lua").exists());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn rejects_incompatible_and_traversal_without_writes() {
        let p = fixture();
        let root = p.join("_forever_");
        let z = p.join("test.zip");
        zip(&z, &[("Test/Test.toc", "## Interface: 120105")]);
        assert!(inspect(&root, &z).unwrap_err().contains("not compatible"));
        zip(
            &z,
            &[
                ("../escape.txt", "bad"),
                ("Test/Test.toc", "## Interface: 16001"),
            ],
        );
        assert!(inspect(&root, &z).is_err());
        assert_eq!(
            fs::read_dir(root.join("Interface/AddOns")).unwrap().count(),
            0
        );
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn handles_wrapped_archive_and_rejects_case_duplicates() {
        let p = fixture();
        let root = p.join("_forever_");
        let z = p.join("test.zip");
        zip(
            &z,
            &[
                ("release/Test/Test.toc", "## Interface: 16001"),
                ("release/Test/test.lua", "ok"),
            ],
        );
        install(&root, &z, &p.join("backup")).unwrap();
        assert!(root.join("Interface/AddOns/Test/test.lua").exists());
        zip(
            &z,
            &[
                ("Test/Test.toc", "## Interface: 16001"),
                ("Test/a.lua", "a"),
                ("Test/A.lua", "b"),
            ],
        );
        assert!(inspect(&root, &z).is_err());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn rolls_back_when_a_later_folder_cannot_be_installed() {
        let p = fixture();
        let root = p.join("_forever_");
        for name in ["A", "B"] {
            fs::create_dir(root.join("Interface/AddOns").join(name)).unwrap();
            fs::write(
                root.join("Interface/AddOns").join(name).join("old.lua"),
                name,
            )
            .unwrap();
        }
        let staging = stage(&root).unwrap();
        fs::create_dir_all(staging.join("new/A")).unwrap();
        fs::write(staging.join("new/A/new.lua"), "new").unwrap();
        let result = exchange(
            &root,
            &staging,
            &["A".into(), "B".into()],
            &p.join("backup"),
            true,
            &[],
        );
        assert!(result.unwrap_err().contains("Original folders restored"));
        for name in ["A", "B"] {
            assert_eq!(
                fs::read_to_string(root.join("Interface/AddOns").join(name).join("old.lua"))
                    .unwrap(),
                name
            );
        }
        assert!(!root.join("Interface/AddOns/A/new.lua").exists());
        fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn skips_only_bundled_optional_modules_for_other_clients() {
        let p = fixture();
        let root = p.join("_forever_");
        let z = p.join("test.zip");
        fs::create_dir(root.join("Interface/AddOns/OtherClientModule")).unwrap();
        fs::write(
            root.join("Interface/AddOns/OtherClientModule/old.lua"),
            "old",
        )
        .unwrap();
        zip(
            &z,
            &[
                ("Main/Main.toc", "## Interface: 16001"),
                (
                    "OtherClientModule/OtherClientModule.toc",
                    "## Interface: 120105\n## Dependencies: Main",
                ),
            ],
        );
        assert_eq!(
            inspect(&root, &z).unwrap()["skipped"][0]["folder"],
            "OtherClientModule"
        );
        install(&root, &z, &p.join("backup")).unwrap();
        assert!(root.join("Interface/AddOns/Main/Main.toc").exists());
        assert!(!root.join("Interface/AddOns/OtherClientModule").exists());
        assert!(p.join("backup/OtherClientModule/old.lua").exists());
        zip(
            &z,
            &[
                (
                    "Main/Main.toc",
                    "## Interface: 16001\n## Dependencies: OtherClientModule",
                ),
                (
                    "OtherClientModule/OtherClientModule.toc",
                    "## Interface: 120105\n## Dependencies: Main",
                ),
            ],
        );
        assert!(inspect(&root, &z).is_err());
        fs::remove_dir_all(p).unwrap();
    }
}
