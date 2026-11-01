use crate::{
    grouping::{self, list, text},
    images, inventory,
    provider::{self, Provider},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub type Result<T> = std::result::Result<T, String>;
pub type Progress = std::sync::Arc<dyn Fn(String) + Send + Sync>;
const COLUMNS: [&str; 7] = [
    "addon",
    "version",
    "compatibility",
    "source",
    "folder",
    "interface",
    "toc",
];
pub fn defaults() -> Value {
    json!({
        "schemaVersion": 1,
        "closeToTray": true,
        "checkUpdatesOnStartup": true,
        "games": [],
        "selectedId": null,
        "columns": ["addon","version"],
        "columnOrder": COLUMNS,
        "bindings": {},
        "packages": {},
        "updates": {},
        "archives": {},
        "sort": {
            "key": "addon",
            "direction": 1,
        },
    })
}
fn folder_name(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', ':'])
}
fn validate(mut state: Value) -> Result<Value> {
    if state["schemaVersion"] != 1 || !state["games"].is_array() {
        return Err("Unsupported or damaged inventory file.".into());
    }
    for g in list(&state["games"]) {
        if !g["id"].is_string()
            || !g["installation"]["root"].is_string()
            || !g["addons"].is_array()
            || list(&g["addons"])
                .iter()
                .any(|a| !folder_name(text(a, "folder")) || !a["name"].is_string())
        {
            return Err("Invalid saved game inventory.".into());
        }
    }
    for key in ["closeToTray", "checkUpdatesOnStartup"] {
        if !state[key].is_boolean() {
            state[key] = defaults()[key].clone();
        }
    }
    for key in ["bindings", "packages", "updates", "archives"] {
        if !state[key].is_object() {
            state[key] = json!({});
        }
    }
    let mut columns: Vec<String> = state["columns"]
        .as_array()
        .cloned()
        .unwrap_or(list(&defaults()["columns"]))
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|s| COLUMNS.contains(s))
        .map(str::to_string)
        .collect();
    let mut seen = BTreeSet::new();
    columns.retain(|v| seen.insert(v.clone()));
    if !columns.iter().any(|s| s == "addon") {
        columns.insert(0, "addon".into());
    }
    if columns == vec!["addon", "version", "compatibility", "source"] {
        columns = vec!["addon".into(), "version".into()];
    }
    state["columns"] = json!(columns);
    let mut order: Vec<Value> = list(&state["columnOrder"]);
    order.extend(COLUMNS.iter().map(|s| json!(s)));
    let mut seen = BTreeSet::new();
    order.retain(|v| {
        v.as_str().is_some_and(|s| COLUMNS.contains(&s)) && seen.insert(v.clone().to_string())
    });
    state["columnOrder"] = json!(order);
    if !COLUMNS.contains(&text(&state["sort"], "key"))
        || !matches!(state["sort"]["direction"].as_i64(), Some(1 | -1))
    {
        state["sort"] = defaults()["sort"].clone();
    }
    Ok(state)
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid storage path.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".write-{}", uuid::Uuid::new_v4()));
    let outcome = (|| {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&temp).map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    let _ = fs::remove_file(temp);
    outcome
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Clone)]
struct Pending {
    id: String,
    root: String,
    archive: PathBuf,
    digest: String,
    plan: Value,
    source: Value,
}
pub struct Service {
    pub dir: PathBuf,
    pub state: Value,
    pub load_error: Option<String>,
    provider: Provider,
    key: String,
    key_saved: bool,
    pending: BTreeMap<String, Pending>,
    remote: Vec<Value>,
    reports: BTreeMap<String, Value>,
    projects: Value,
    index: Value,
    progress: Progress,
}
impl Service {
    pub fn new(dir: PathBuf, progress: Progress) -> Result<Self> {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let read = fs::read(dir.join("inventory.json"));
        let (state, load_error) = match read {
            Ok(bytes) => match serde_json::from_slice(&bytes)
                .map_err(|e| e.to_string())
                .and_then(validate)
            {
                Ok(s) => (s, None),
                Err(e) => (
                    defaults(),
                    Some(format!(
                        "Saved inventory could not be loaded. Existing file was preserved. {e}"
                    )),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (defaults(), None),
            Err(e) => (defaults(), Some(e.to_string())),
        };
        let read_json = |path: PathBuf| {
            fs::read(path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .unwrap_or(json!({}))
        };
        let projects = read_json(dir.join("cache/projects.json"));
        let mut index = read_json(dir.join("cache/icons/index.json"));
        if !index["entries"].is_object() {
            index = json!({"schemaVersion":1,"entries":{}});
        }
        #[allow(unused_mut)]
        let mut service = Self {
            dir,
            state,
            load_error,
            provider: Provider::new()?,
            key: String::new(),
            key_saved: false,
            pending: BTreeMap::new(),
            remote: Vec::new(),
            reports: BTreeMap::new(),
            projects,
            index,
            progress,
        };
        #[cfg(feature = "desktop")]
        if let Ok(entry) = service.key_entry() {
            if let Ok(key) = entry.get_password() {
                service.key = key;
                service.key_saved = true;
            }
        }
        #[cfg(all(feature = "desktop", windows))]
        if service.key.is_empty() {
            if let Some(key) = crate::migration::legacy_key(&service.dir) {
                if let Ok(entry) = service.key_entry() {
                    if entry.set_password(&key).is_ok() {
                        service.key = key;
                        service.key_saved = true;
                        let _ = fs::remove_file(service.dir.join("credentials.json"));
                    }
                }
            }
        }
        Ok(service)
    }
    #[cfg(feature = "desktop")]
    fn key_entry(&self) -> Result<keyring::Entry> {
        let account = format!(
            "curseforge-{:x}",
            Sha256::digest(self.dir.to_string_lossy().as_bytes())
        );
        keyring::Entry::new("io.wowaddons.desktop", &account).map_err(|e| e.to_string())
    }
    fn stage(&self, s: impl Into<String>) {
        (self.progress)(s.into());
    }
    fn persist(&mut self) -> Result<Value> {
        let checked = validate(self.state.clone())?;
        atomic(
            &self.dir.join("inventory.json"),
            &serde_json::to_vec_pretty(&checked).map_err(|e| e.to_string())?,
        )?;
        self.state = checked;
        Ok(self.state.clone())
    }
    fn game(&self, id: &str) -> Result<Value> {
        list(&self.state["games"])
            .into_iter()
            .find(|g| g["id"] == id)
            .ok_or("Game profile was not found.".into())
    }
    fn groups(&self, id: &str) -> Result<Vec<Value>> {
        let g = self.game(id)?;
        Ok(grouping::groups(
            &g["addons"],
            &self.state["bindings"][id],
            &self.state["packages"][id],
        ))
    }
    fn group(&self, id: &str, folder: &str) -> Result<Value> {
        self.groups(id)?
            .into_iter()
            .find(|g| list(&g["members"]).iter().any(|a| a["folder"] == folder))
            .ok_or("Addon was not found.".into())
    }
    fn scan(&mut self, id: &str) -> Result<()> {
        let current = self.game(id)?;
        self.stage("Scanning addon folders…");
        let mut scan = inventory::run("scan", text(&current["installation"], "root"), &[])?;
        scan["id"] = json!(id);
        if let Some(g) = self.state["games"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|g| g["id"] == id)
        {
            *g = scan;
        }
        self.persist()?;
        Ok(())
    }
    fn valid_project(p: &Value) -> bool {
        p["id"].is_u64()
            && p["gameId"] == 1
            && provider::parse_url(text(&p["links"], "websiteUrl"))
                .is_ok_and(|(slug, id, _)| slug == text(p, "slug") && id.is_none())
    }
    fn cache_project(&mut self, p: &Value) -> Result<()> {
        if !Self::valid_project(p) {
            return Err("Invalid CurseForge project metadata.".into());
        }
        self.projects[p["id"].to_string()] = json!({
            "id": p["id"],
            "gameId": 1,
            "name": p["name"],
            "slug": p["slug"],
            "links": p["links"],
            "logo": p["logo"],
        });
        atomic(
            &self.dir.join("cache/projects.json"),
            &serde_json::to_vec(&self.projects).unwrap(),
        )
    }
    fn project_for_group(&mut self, id: &str, group: &Value) -> Result<Option<Value>> {
        let bindings = &self.state["bindings"][id];
        let url = grouping::link(group, bindings);
        let icon_id = grouping::identity(group, bindings);
        let wanted = list(&group["members"])
            .iter()
            .find_map(|a| a["curseProjectId"].as_u64())
            .or_else(|| self.index["entries"][&icon_id]["projectId"].as_u64());
        let cached = if let Some(ref u) = url {
            let (slug, _, _) = provider::parse_url(u)?;
            self.projects
                .as_object()
                .and_then(|m| {
                    m.values()
                        .find(|p| text(p, "slug") == slug && Self::valid_project(p))
                })
                .cloned()
        } else {
            wanted
                .and_then(|n| self.projects.get(n.to_string()).cloned())
                .filter(Self::valid_project)
        };
        if cached.is_some() {
            return Ok(cached);
        }
        if self.key.is_empty() {
            return Ok(None);
        }
        let project = if let Some(ref url) = url {
            Some(self.provider.project(url, &self.key)?)
        } else if let Some(n) = wanted {
            Some(
                self.provider
                    .request(&self.key, &format!("mods/{n}"), None)?,
            )
        } else {
            None
        };
        if let Some(ref p) = project {
            self.cache_project(p)?;
        }
        Ok(project)
    }
    fn bind_project(&mut self, id: &str, group: &Value, project: &Value) -> Result<()> {
        if grouping::link(group, &self.state["bindings"][id]).is_some() {
            return Ok(());
        }
        let before = grouping::identity(group, &self.state["bindings"][id]);
        let folder = text(group, "folder");
        self.state["bindings"][id][folder] = json!({
            "url": project["links"]["websiteUrl"],
            "slug": project["slug"],
            "projectId": project["id"],
            "verified": true,
        });
        let after = grouping::identity(group, &self.state["bindings"][id]);
        let from = self.dir.join(format!("cache/icons/{before}.png"));
        let to = self.dir.join(format!("cache/icons/{after}.png"));
        if before != after && from.exists() && !to.exists() {
            let _ = fs::copy(from, to);
        }
        Ok(())
    }
    pub fn enrich(&mut self, id: &str, folders: Option<&[String]>, force: bool) -> Result<Value> {
        let game = self.game(id)?;
        let groups = self.groups(id)?;
        let mut report = json!({"downloaded":0,"local":0,"cached":0,"missing":[],"errors":[]});
        self.stage("Linking CurseForge projects…");
        let dbm_prefix = regex::Regex::new(r"^<DBM [^>]+>\s*").unwrap();
        for group in groups {
            if folders.is_some_and(|fs| {
                !list(&group["members"])
                    .iter()
                    .any(|a| fs.iter().any(|f| a["folder"] == *f))
            }) {
                continue;
            }
            let project = match self.project_for_group(id, &group) {
                Ok(p) => p,
                Err(e) => {
                    report["errors"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(format!("{}: {e}", text(&group, "name"))));
                    None
                }
            };
            if let Some(ref project) = project {
                if let Err(e) = self.bind_project(id, &group, project) {
                    report["errors"].as_array_mut().unwrap().push(json!(e));
                }
            }
            let identity = grouping::identity(&group, &self.state["bindings"][id]);
            let file = self.dir.join(format!("cache/icons/{identity}.png"));
            if !force && file.exists() {
                report["cached"] = json!(report["cached"].as_u64().unwrap() + 1);
                continue;
            }
            self.stage(format!("Loading icon: {}", text(&group, "name")));
            if let Some(png) = images::local(&game, &group) {
                atomic(&file, &png)?;
                self.index["entries"][&identity] =
                    json!({"source":"local","folder":group["folder"],"cachedAt":now()});
                report["local"] = json!(report["local"].as_u64().unwrap() + 1);
                continue;
            }
            let fetched = (|| -> Result<bool> {
                let mut project = project;
                if project.is_none() && !self.key.is_empty() {
                    let raw = text(&group, "name");
                    let search = dbm_prefix.replace(raw, "").to_string();
                    let route = format!(
                        "mods/search?gameId=1&pageSize=50&searchFilter={}",
                        url::form_urlencoded::byte_serialize(search.as_bytes()).collect::<String>()
                    );
                    let records = self.provider.request(&self.key, &route, None)?;
                    let normalize = |s: &str| {
                        s.to_lowercase()
                            .chars()
                            .filter(|c| c.is_alphanumeric())
                            .collect::<String>()
                    };
                    let exact: Vec<_> = list(&records)
                        .into_iter()
                        .filter(|p| {
                            [raw, text(&group, "folder"), &search].iter().any(|n| {
                                let x = normalize(n);
                                let y = normalize(text(p, "name"));
                                x == y
                                    || x == y.trim_end_matches("reborn")
                                    || x.trim_start_matches("dbm") == y.trim_start_matches("dbm")
                            })
                        })
                        .collect();
                    if exact.len() == 1 {
                        self.cache_project(&exact[0])?;
                        self.bind_project(id, &group, &exact[0])?;
                        project = Some(exact[0].clone());
                    }
                }
                let Some(project) = project else {
                    return Ok(false);
                };
                let url = if !text(&project["logo"], "thumbnailUrl").is_empty() {
                    text(&project["logo"], "thumbnailUrl")
                } else {
                    text(&project["logo"], "url")
                };
                if url.is_empty() {
                    return Ok(false);
                }
                let r = self.provider.response(url, true)?;
                if !r.status().is_success() {
                    return Err(format!("Icon download returned HTTP {}.", r.status()));
                }
                let mut bytes = Vec::new();
                r.take(2 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                let png = images::png(&bytes, "").ok_or("Icon could not be decoded.")?;
                let identity = grouping::identity(&group, &self.state["bindings"][id]);
                atomic(&self.dir.join(format!("cache/icons/{identity}.png")), &png)?;
                self.index["entries"][&identity] = json!({
                    "source": "curseforge",
                    "projectId": project["id"],
                    "name": project["name"],
                    "slug": project["slug"],
                    "cachedAt": now(),
                });
                Ok(true)
            })();
            match fetched {
                Ok(true) => {
                    report["downloaded"] = json!(report["downloaded"].as_u64().unwrap() + 1)
                }
                Ok(false) => report["missing"]
                    .as_array_mut()
                    .unwrap()
                    .push(group["name"].clone()),
                Err(e) => report["errors"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(format!("{}: {e}", text(&group, "name")))),
            }
        }
        atomic(
            &self.dir.join("cache/icons/index.json"),
            &serde_json::to_vec(&self.index).unwrap(),
        )?;
        self.persist()?;
        self.reports.insert(id.into(), report.clone());
        Ok(report)
    }
    fn export_list(&self, id: &str) -> Result<Value> {
        let game = self.game(id)?;
        let mut addons = Vec::new();
        for group in self.groups(id)? {
            let package = list(&self.state["packages"][id]).into_iter().find(|p| {
                list(&p["folders"]).contains(&group["folder"])
                    && p["installedVersion"] == group["version"]
            });
            let mut url = grouping::link(&group, &self.state["bindings"][id]);
            if let Some(p) = package {
                if let (Some(base), Some(file)) = (p["url"].as_str(), p["fileId"].as_u64()) {
                    url = Some(format!("{base}/files/{file}"));
                }
            }
            addons.push(json!({"name":group["name"],"version":group["version"],"url":url}));
        }
        let document = json!({
            "format":"wow-addons-list", "schemaVersion":1,
            "flavor":game["installation"]["flavor"], "addons":addons,
        });
        Self::validate_list(&document, text(&game["installation"], "flavor"))?;
        Ok(document)
    }
    fn validate_list(document: &Value, flavor: &str) -> Result<()> {
        if document["format"] != "wow-addons-list" || document["schemaVersion"] != 1 {
            return Err("Unsupported addon list format or version.".into());
        }
        if flavor == "Custom" || flavor.is_empty() || document["flavor"] != flavor {
            return Err("This addon list is for a different WoW client. Select the matching client (for example Forever or Retail). Custom clients are not supported.".into());
        }
        let addons = document["addons"].as_array().ok_or("Invalid addon list.")?;
        if addons.len() > 500 {
            return Err("Addon lists may contain at most 500 entries.".into());
        }
        let mut seen = BTreeSet::new();
        for addon in addons {
            if text(addon, "name").is_empty()
                || text(addon, "name").len() > 300
                || !addon["version"].is_string()
            {
                return Err("Invalid addon list entry.".into());
            }
            if !addon["url"].is_null() {
                let url = addon["url"].as_str().ok_or("Invalid addon URL.")?;
                provider::parse_url(url)?;
                if !seen.insert(url) {
                    return Err("Duplicate addon URL in list.".into());
                }
            }
        }
        Ok(())
    }
    fn install_list(&mut self, id: &str, document: &Value) -> Result<Value> {
        let game = self.game(id)?;
        Self::validate_list(document, text(&game["installation"], "flavor"))?;
        let mut installed = 0;
        let mut failures = Vec::new();
        for addon in list(&document["addons"]) {
            let outcome: Result<()> = (|| {
                let url = addon["url"]
                    .as_str()
                    .ok_or("No download source; install manually.")?;
                let plan = self.prepare_url(id, url)?;
                if plan["blocked"] == true {
                    return Err("Provider requires a manual browser download.".into());
                }
                self.install(text(&plan, "token"))?;
                Ok(())
            })();
            match outcome {
                Ok(()) => installed += 1,
                Err(e) => failures.push(json!({"name":addon["name"],"reason":e})),
            }
        }
        Ok(json!({"state":self.state,"installed":installed,"failures":failures}))
    }
    fn select_list(&self, id: &str, selection: &Value) -> Result<Value> {
        let mut document = self.export_list(id)?;
        let indices = selection.as_array().ok_or("Invalid addon selection.")?;
        if indices.is_empty() {
            return Err("Select at least one addon.".into());
        }
        let all = list(&document["addons"]);
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for index in indices {
            let index = index
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&n| n < all.len())
                .ok_or("Addon selection changed. Open export again.")?;
            if !seen.insert(index) {
                return Err("Duplicate addon selection.".into());
            }
            selected.push(all[index].clone());
        }
        document["addons"] = json!(selected);
        Ok(document)
    }
    fn list_dialog(&self, id: &str, selection: Option<&Value>) -> Result<Value> {
        #[cfg(feature = "desktop")]
        {
            let game = self.game(id)?;
            let document = if let Some(selection) = selection {
                Some(self.select_list(id, selection)?)
            } else {
                None
            };
            let dialog = rfd::FileDialog::new().add_filter("Addon list JSON", &["json"]);
            let chosen = if document.is_some() {
                dialog
                    .set_title("Export addon list")
                    .set_file_name("wow-addons.json")
                    .save_file()
            } else {
                dialog.set_title("Import addon list").pick_file()
            };
            let Some(path) = chosen else {
                return Ok(Value::Null);
            };
            if let Some(document) = document {
                fs::write(
                    &path,
                    serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                Ok(json!({"count":list(&document["addons"]).len()}))
            } else {
                let mut bytes = Vec::new();
                fs::File::open(path)
                    .map_err(|e| e.to_string())?
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 1024 * 1024 {
                    return Err("Addon list exceeds 1 MB.".into());
                }
                let document: Value = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Invalid addon list JSON: {e}"))?;
                Self::validate_list(&document, text(&game["installation"], "flavor"))?;
                Ok(document)
            }
        }
        #[cfg(not(feature = "desktop"))]
        {
            let _ = (id, selection);
            Err("File dialogs require the desktop feature.".into())
        }
    }
    fn prepare_archive(&mut self, id: &str, input: &str, mut source: Value) -> Result<Value> {
        let current = self.game(id)?;
        let input = Path::new(input);
        if input
            .extension()
            .and_then(|s| s.to_str())
            .is_none_or(|s| !s.eq_ignore_ascii_case("zip"))
        {
            return Err("Choose a .zip addon archive.".into());
        }
        let dir = self.dir.join("archives");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let temp = dir.join(format!(".import-{}.zip", uuid::Uuid::new_v4()));
        if fs::metadata(input).map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024 {
            return Err("ZIP exceeds 256 MB.".into());
        }
        fs::copy(input, &temp).map_err(|e| e.to_string())?;
        let outcome = (|| {
            let hashes = provider::hashes(&temp)?;
            let digest = text(&hashes, "sha256").to_string();
            let archive = dir.join(format!("{digest}.zip"));
            if !archive.exists() {
                fs::rename(&temp, &archive).map_err(|e| e.to_string())?;
            }
            self.stage("Checking ZIP compatibility…");
            let plan = inventory::run(
                "inspect-zip",
                text(&current["installation"], "root"),
                &[archive.to_string_lossy().into_owned()],
            )?;
            if source.is_null() {
                for r in &self.remote {
                    if list(&r["hashes"]).iter().any(|h| {
                        let key = match h["algo"].as_u64() {
                            Some(1) => "sha1",
                            Some(2) => "md5",
                            _ => return false,
                        };
                        text(&hashes, key).eq_ignore_ascii_case(text(h, "value"))
                    }) {
                        source = r["source"].clone();
                        break;
                    }
                }
            }
            let token = uuid::Uuid::new_v4().to_string();
            self.pending.insert(
                token.clone(),
                Pending {
                    id: id.into(),
                    root: text(&current["installation"], "root").into(),
                    archive,
                    digest,
                    plan: plan.clone(),
                    source: source.clone(),
                },
            );
            let mut result = plan;
            result["token"] = json!(token);
            result["source"] = source;
            Ok(result)
        })();
        let _ = fs::remove_file(temp);
        outcome
    }
    fn prepare_url(&mut self, id: &str, url: &str) -> Result<Value> {
        let game = self.game(id)?;
        self.stage("Looking up CurseForge release…");
        let selected =
            self.provider
                .selection(url, text(&game["installation"], "flavor"), &self.key)?;
        self.stage("Downloading addon ZIP…");
        if let Some(archive) = self.provider.download(
            &selected,
            &self.dir.join("archives"),
            self.progress.as_ref(),
        )? {
            self.prepare_archive(id, &archive.to_string_lossy(), selected["source"].clone())
        } else {
            self.remote
                .push(json!({"source":selected["source"],"hashes":selected["file"]["hashes"]}));
            if self.remote.len() > 20 {
                self.remote.remove(0);
            }
            Ok(json!({
                "blocked": true,
                "downloadPage": selected["downloadPage"],
                "source": selected["source"],
                "reason": "Automatic download needs browser interaction. Open the download page and import its ZIP.",
            }))
        }
    }
    fn install(&mut self, token: &str) -> Result<Value> {
        let p = self
            .pending
            .remove(token)
            .ok_or("Import expired. Choose the ZIP again.")?;
        let current = self.game(&p.id)?;
        if text(&current["installation"], "root") != p.root {
            return Err("Game profile changed.".into());
        }
        let fresh = inventory::run(
            "inspect-zip",
            &p.root,
            &[p.archive.to_string_lossy().into_owned()],
        )?;
        for folder in list(&fresh["packages"])
            .into_iter()
            .chain(list(&fresh["skipped"]))
        {
            if folder["exists"] == true
                && !list(&p.plan["packages"])
                    .into_iter()
                    .chain(list(&p.plan["skipped"]))
                    .any(|old| old["folder"] == folder["folder"] && old["exists"] == true)
            {
                return Err("An addon folder appeared after validation. Import again to confirm replacement.".into());
            }
        }
        let backup = self.dir.join("backups").join(&p.id).join(format!(
            "{}-{}",
            now(),
            uuid::Uuid::new_v4()
        ));
        self.stage("Backing up and installing addon folders…");
        let installed = inventory::run(
            "install-zip",
            &p.root,
            &[
                p.archive.to_string_lossy().into_owned(),
                backup.to_string_lossy().into_owned(),
            ],
        )?;
        self.scan(&p.id)?;
        let game = self.game(&p.id)?;
        self.state["packages"][&p.id] = grouping::record(
            &self.state["packages"][&p.id],
            &game["addons"],
            &installed["installed"],
            &p.digest,
            &p.source,
        );
        for folder in list(&installed["installed"]) {
            self.state["archives"][&p.id][folder.as_str().unwrap()] = json!(p.digest);
        }
        let names = list(&installed["installed"]);
        let old = self.state["updates"][&p.id]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for f in old.keys() {
            if names.contains(&json!(f)) {
                self.state["updates"][&p.id]
                    .as_object_mut()
                    .unwrap()
                    .remove(f);
            }
        }
        if !p.source.is_null() {
            let record = list(&self.state["packages"][&p.id])
                .into_iter()
                .find(|v| v["projectId"] == p.source["id"])
                .ok_or("Installed package record missing.")?;
            let primary = text(&record, "primary");
            self.state["bindings"][&p.id][primary] = json!({
                "url": p.source["url"],
                "slug": p.source["slug"],
                "projectId": p.source["id"],
                "verified": true,
            });
            self.state["updates"][&p.id][primary] = json!({
                "status": if p.source["isLatestStable"]==true{"current"}else{"unchecked"},
                "checkedAt": now(),
                "latestVersion": p.source["fileVersion"],
                "fileId": p.source["fileId"],
                "projectId": p.source["id"],
                "projectUrl": p.source["url"],
                "downloadPage": format!("{}/download/{}",text(&p.source,"url"),p.source["fileId"]),
                "reason": "Installed file recorded. Check updates to compare with stable.",
            });
        }
        self.persist()?;
        let folders: Vec<String> = names
            .iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect();
        let _ = self.enrich(&p.id, Some(&folders), false);
        Ok(json!({"state":self.state,"installed":installed}))
    }
    fn check_updates(&mut self, id: &str, folder: Option<&str>) -> Result<Value> {
        if self.key.is_empty() {
            return Err("Configure your CurseForge API key in Settings first.".into());
        }
        let g = self.game(id)?;
        let groups: Vec<_> = self
            .groups(id)?
            .into_iter()
            .filter(|g| folder.is_none_or(|f| list(&g["members"]).iter().any(|a| a["folder"] == f)))
            .collect();
        let mut counts = [0u64; 3];
        for (n, group) in groups.iter().enumerate() {
            self.stage(format!(
                "Checking updates: {} ({}/{})",
                text(group, "name"),
                n + 1,
                groups.len()
            ));
            let checked = (|| -> Result<Value> {
                if grouping::link(group, &self.state["bindings"][id]).is_none() {
                    if let Some(p) = self.project_for_group(id, group)? {
                        self.bind_project(id, group, &p)?;
                    }
                }
                let Some(url) = grouping::link(group, &self.state["bindings"][id]) else {
                    return Ok(json!({
                        "status": "unlinked",
                        "checkedAt": now(),
                        "reason": "No CurseForge project URL is linked.",
                    }));
                };
                let project = self.provider.project(&url, &self.key)?;
                self.cache_project(&project)?;
                let latest = self.provider.latest(
                    &project,
                    text(&g["installation"], "flavor"),
                    &self.key,
                )?;
                let manifest = list(&self.state["packages"][id]).into_iter().find(|p| {
                    p["projectId"] == project["id"]
                        && list(&p["folders"])
                            .iter()
                            .any(|f| list(&group["members"]).iter().any(|a| a["folder"] == *f))
                });
                let known = manifest
                    .as_ref()
                    .filter(|p| {
                        text(p, "installedVersion").is_empty()
                            || p["installedVersion"] == group["version"]
                    })
                    .and_then(|p| p["fileId"].as_u64());
                let mut installed = Value::Null;
                if let Some(file_id) = known {
                    if Some(file_id) != latest["id"].as_u64() {
                        installed = list(&project["latestFiles"])
                            .into_iter()
                            .find(|f| f["id"] == file_id)
                            .or_else(|| {
                                self.provider
                                    .request(
                                        &self.key,
                                        &format!("mods/{}/files/{file_id}", project["id"]),
                                        None,
                                    )
                                    .ok()
                            })
                            .unwrap_or(Value::Null);
                    }
                }
                let digest = list(&group["members"])
                    .iter()
                    .find_map(|a| self.state["archives"][id][text(a, "folder")].as_str())
                    .map(str::to_string);
                let hashes = if known.is_none()
                    && manifest.as_ref().is_none_or(|m| {
                        text(m, "installedVersion").is_empty()
                            || m["installedVersion"] == group["version"]
                    }) {
                    digest
                        .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
                        .and_then(|d| {
                            provider::hashes(&self.dir.join(format!("archives/{d}.zip"))).ok()
                        })
                        .unwrap_or(Value::Null)
                } else {
                    Value::Null
                };
                let mut result =
                    provider::assess(text(group, "version"), known, &latest, &installed, &hashes);
                result["checkedAt"] = json!(now());
                result["latestVersion"] = latest["displayName"].clone();
                result["fileId"] = latest["id"].clone();
                result["projectId"] = project["id"].clone();
                result["projectUrl"] = json!(url);
                result["downloadPage"] = json!(format!("{url}/download/{}", latest["id"]));
                result["installedVersion"] = group["version"].clone();
                Ok(result)
            })();
            let result =
                checked.unwrap_or_else(|e| json!({"status":"error","checkedAt":now(),"reason":e}));
            match text(&result, "status") {
                "available" => counts[0] += 1,
                "unverified" => counts[1] += 1,
                "error" => counts[2] += 1,
                _ => {}
            }
            self.state["updates"][id][text(group, "folder")] = result;
        }
        self.persist()?;
        Ok(json!({
            "state": self.state,
            "summary": {
                "checked": groups.len(),
                "available": counts[0],
                "unverified": counts[1],
                "errors": counts[2],
            },
        }))
    }

    fn key_status(&self) -> Value {
        json!({
            "configured": !self.key.is_empty(),
            "saved": self.key_saved||self.dir.join("credentials.json").exists(),
            "migrationRequired": !self.key_saved&&self.dir.join("credentials.json").exists(),
        })
    }
    fn save_key(&mut self, value: &str) -> Result<Value> {
        let candidate = value.trim();
        if candidate.is_empty() || candidate.len() > 512 {
            return Err("Enter your CurseForge API key.".into());
        }
        #[allow(unused_mut)]
        let mut saved = false;
        #[cfg(feature = "desktop")]
        {
            if let Ok(entry) = self.key_entry() {
                saved = entry.set_password(candidate).is_ok();
            }
        }
        self.key = candidate.into();
        self.key_saved = saved;
        if saved && self.dir.join("credentials.json").exists() {
            fs::remove_file(self.dir.join("credentials.json")).map_err(|e| e.to_string())?;
        }
        Ok(json!({"saved":saved}))
    }
    fn clear_key(&mut self) -> Result<Value> {
        #[cfg(feature = "desktop")]
        {
            if let Ok(entry) = self.key_entry() {
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) if self.key_saved => {
                        return Err(format!("Could not clear the saved key: {e}"))
                    }
                    Err(_) => {}
                }
            }
        }
        for name in ["credentials.json", "credentials.json.tmp"] {
            match fs::remove_file(self.dir.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        self.key.clear();
        self.key_saved = false;
        Ok(self.key_status())
    }
    fn client_icon(&self, id: &str) -> Result<Value> {
        let g = self.game(id)?;
        let root = text(&g["installation"], "root");
        let hash = format!("{:x}", Sha256::digest(root.as_bytes()));
        let cache = self.dir.join(format!("cache/clients/{hash}.png"));
        if let Some(data) = images::data_url(&cache) {
            return Ok(json!(data));
        }
        #[cfg(all(feature = "desktop", any(windows, target_os = "macos")))]
        {
            let root = Path::new(root);
            let parent = root.parent().unwrap_or(root);
            let names = if cfg!(target_os = "macos") {
                vec![
                    root.join("World of Warcraft.app"),
                    root.join("World of Warcraft Classic.app"),
                    parent.join("World of Warcraft Launcher.app"),
                ]
            } else {
                vec![
                    root.join("Wow.exe"),
                    root.join("WowClassic.exe"),
                    root.join("WoW.exe"),
                    parent.join("World of Warcraft Launcher.exe"),
                    parent.join("World of Warcraft Launcher"),
                ]
            };
            for file in names {
                if file.exists() {
                    if let Ok(icon) = file_icon_provider::get_file_icon(
                        PathBuf::from(shell_path(&file.to_string_lossy())),
                        64,
                    ) {
                        if let Some(image) =
                            image::RgbaImage::from_raw(icon.width, icon.height, icon.pixels)
                        {
                            let mut out = std::io::Cursor::new(Vec::new());
                            if image::DynamicImage::ImageRgba8(image)
                                .write_to(&mut out, image::ImageFormat::Png)
                                .is_ok()
                            {
                                atomic(&cache, &out.into_inner())?;
                                return Ok(json!(images::data_url(&cache)));
                            }
                        }
                    }
                }
            }
        }
        Ok(Value::Null)
    }
    fn icon(&self, id: &str, folder: &str) -> Result<Value> {
        let g = self.game(id)?;
        let group = self.group(id, folder)?;
        let hash = grouping::identity(&group, &self.state["bindings"][id]);
        let file = self.dir.join(format!("cache/icons/{hash}.png"));
        if let Some(data) = images::data_url(&file) {
            return Ok(json!(data));
        }
        if let Some(bytes) = images::local(&g, &group) {
            use base64::Engine;
            return Ok(json!(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )));
        }
        Ok(Value::Null)
    }
    pub fn dispatch(&mut self, method: &str, args: &[Value]) -> Result<Value> {
        let arg = |i: usize| -> Result<String> {
            args.get(i)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or("Missing command argument.".into())
        };
        if method == "load" {
            return Ok(json!({
                "state": self.state,
                "error": self.load_error,
                "cachePath": self.dir.join("inventory.json"),
            }));
        }
        if let Some(ref e) = self.load_error {
            return Err(e.clone());
        }
        match method {
            "addon-list" => self.export_list(&arg(0)?),
            "select-addon-list" => {
                self.select_list(&arg(0)?, args.get(1).ok_or("Missing addon selection.")?)
            }
            "export-list" => self.list_dialog(
                &arg(0)?,
                Some(args.get(1).ok_or("Missing addon selection.")?),
            ),
            "choose-list" => self.list_dialog(&arg(0)?, None),
            "install-list" => {
                self.install_list(&arg(0)?, args.get(1).ok_or("Missing addon list.")?)
            }
            "about" => Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "repository": "https://github.com/h2u/wow-addons",
                "support": "https://buymeacoffee.com/edgars.spirins",
            })),
            "open-about-link" => {
                let url = match arg(0)?.as_str() {
                    "repository" => "https://github.com/h2u/wow-addons",
                    "support" => "https://buymeacoffee.com/edgars.spirins",
                    "buildGuide" => "https://github.com/h2u/wow-addons#build-from-source",
                    "apiConsole" => "https://console.curseforge.com/",
                    _ => return Err("Unknown link.".into()),
                };
                open::that(url).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "key-status" => Ok(self.key_status()),
            "save-api-key" => self.save_key(&arg(0)?),
            "clear-api-key" => self.clear_key(),
            "discover" => inventory::run("discover", &arg(0)?, &[]),
            "add" => {
                let roots = args
                    .first()
                    .and_then(Value::as_array)
                    .ok_or("Choose at least one game.")?;
                if roots.is_empty() || roots.len() > 30 {
                    return Err("Choose 1–30 game clients.".into());
                }
                let mut added = Vec::new();
                for root in roots {
                    let root = root.as_str().ok_or("Invalid game path.")?;
                    let mut scan = inventory::run("scan", root, &[])?;
                    let same = |a: &str, b: &str| {
                        if cfg!(windows) {
                            a.eq_ignore_ascii_case(b)
                        } else {
                            a == b
                        }
                    };
                    if list(&self.state["games"]).iter().any(|g| {
                        same(
                            text(&g["installation"], "root"),
                            text(&scan["installation"], "root"),
                        )
                    }) {
                        continue;
                    }
                    let id = uuid::Uuid::new_v4().to_string();
                    scan["id"] = json!(id);
                    self.state["games"].as_array_mut().unwrap().push(scan);
                    self.state["selectedId"] = json!(id);
                    added.push(id);
                }
                self.persist()?;
                for id in added {
                    let _ = self.enrich(&id, None, false);
                }
                Ok(self.state.clone())
            }
            "select" => {
                let id = arg(0)?;
                self.game(&id)?;
                self.state["selectedId"] = json!(id);
                self.persist()
            }
            "rescan" => {
                let id = arg(0)?;
                self.scan(&id)?;
                self.enrich(&id, None, false)?;
                Ok(self.state.clone())
            }
            "remove" => {
                let id = arg(0)?;
                self.game(&id)?;
                self.state["games"] = json!(list(&self.state["games"])
                    .into_iter()
                    .filter(|g| g["id"] != id)
                    .collect::<Vec<_>>());
                for key in ["bindings", "packages", "updates", "archives"] {
                    if let Some(map) = self.state[key].as_object_mut() {
                        map.remove(&id);
                    }
                }
                if self.state["selectedId"] == id {
                    self.state["selectedId"] = list(&self.state["games"])
                        .first()
                        .map(|g| g["id"].clone())
                        .unwrap_or(Value::Null);
                }
                self.persist()
            }
            "preferences" => {
                let data = args.first().ok_or("Missing preferences.")?;
                for field in [
                    "columns",
                    "columnOrder",
                    "sort",
                    "closeToTray",
                    "checkUpdatesOnStartup",
                ] {
                    if data.get(field).is_some() {
                        self.state[field] = data[field].clone();
                    }
                }
                self.persist()
            }
            "bind" => {
                let (id, folder, input) = (arg(0)?, arg(1)?, arg(2)?);
                self.group(&id, &folder)?;
                let input = input.trim();
                if input.is_empty() {
                    if let Some(map) = self.state["bindings"][&id].as_object_mut() {
                        map.remove(&folder);
                    }
                } else {
                    let (slug, file, url) = provider::parse_url(input)?;
                    if file.is_some() {
                        return Err("Use a project URL rather than a file URL.".into());
                    }
                    self.state["bindings"][&id][&folder] = json!({"url":url,"slug":slug});
                }
                self.persist()
            }
            "icon" => self.icon(&arg(0)?, &arg(1)?),
            "client-icon" => self.client_icon(&arg(0)?),
            "get-icon" => self.enrich(&arg(0)?, Some(&[arg(1)?]), true),
            "report" => Ok(self.reports.get(&arg(0)?).cloned().unwrap_or(Value::Null)),
            "resolve-source" => {
                let id = arg(0)?;
                for group in self.groups(&id)? {
                    if let Some(project) = self.project_for_group(&id, &group)? {
                        self.bind_project(&id, &group, &project)?;
                    }
                }
                self.persist()
            }
            "view-source" => {
                let id = arg(0)?;
                let group = self.group(&id, &arg(1)?)?;
                if grouping::link(&group, &self.state["bindings"][&id]).is_none() {
                    if let Some(project) = self.project_for_group(&id, &group)? {
                        self.bind_project(&id, &group, &project)?;
                        self.persist()?;
                    }
                }
                let url = grouping::link(&group, &self.state["bindings"][&id])
                    .ok_or("No CurseForge page is linked.")?;
                let (_, _, url) = provider::parse_url(&url)?;
                open::that(url).map_err(|e| e.to_string())?;
                Ok(self.state.clone())
            }
            "show-folder" => {
                let id = arg(0)?;
                let folder = arg(1)?;
                let game = self.game(&id)?;
                self.group(&id, &folder)?;
                if !folder_name(&folder) {
                    return Err("Invalid folder.".into());
                }
                let base = fs::canonicalize(text(&game["installation"], "addonsPath"))
                    .map_err(|e| e.to_string())?;
                let target = fs::canonicalize(base.join(&folder)).map_err(|e| e.to_string())?;
                if !target.starts_with(base) {
                    return Err("Folder points outside AddOns.".into());
                }
                open::that(shell_path(&target.to_string_lossy())).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "inspect-zip" => self.prepare_archive(&arg(0)?, &arg(1)?, Value::Null),
            "prepare-url" => self.prepare_url(&arg(0)?, &arg(1)?),
            "install-zip" => self.install(&arg(0)?),
            "cancel-import" => {
                self.pending.remove(&arg(0)?);
                Ok(Value::Null)
            }
            "check-updates" => self.check_updates(&arg(0)?, args.get(1).and_then(Value::as_str)),
            "url-releases" => {
                let id = arg(0)?;
                let url = arg(1)?;
                let g = self.game(&id)?;
                let flavor = text(&g["installation"], "flavor");
                let index = match args.get(2) {
                    None | Some(Value::Null) => 0,
                    Some(v) => v.as_u64().ok_or("Invalid release page.")?,
                };
                let mut result = self.provider.releases(&url, flavor, &self.key, index)?;
                result["flavor"] = json!(flavor);
                Ok(result)
            }
            "releases" => {
                let id = arg(0)?;
                let group = self.group(&id, &arg(1)?)?;
                let url = grouping::link(&group, &self.state["bindings"][&id])
                    .ok_or("Link this addon to CurseForge first.")?;
                let g = self.game(&id)?;
                let index = match args.get(2) {
                    None | Some(Value::Null) => 0,
                    Some(v) => v.as_u64().ok_or("Invalid release page.")?,
                };
                let mut result = self.provider.releases(
                    &url,
                    text(&g["installation"], "flavor"),
                    &self.key,
                    index,
                )?;
                let manifest = list(&self.state["packages"][&id]).into_iter().find(|p| {
                    list(&p["folders"])
                        .iter()
                        .any(|f| list(&group["members"]).iter().any(|a| a["folder"] == *f))
                });
                result["installedFileId"] = manifest
                    .filter(|p| p["installedVersion"] == group["version"])
                    .map(|p| p["fileId"].clone())
                    .unwrap_or(Value::Null);
                result["installedVersion"] = group["version"].clone();
                result["flavor"] = g["installation"]["flavor"].clone();
                Ok(result)
            }
            "open-download" => {
                let (_, id, url) = provider::parse_url(&arg(0)?)?;
                let url = if let Some(id) = id {
                    format!("{url}/download/{id}")
                } else {
                    url
                };
                open::that(url).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "remove-addon" => {
                let id = arg(0)?;
                let group = self.group(&id, &arg(1)?)?;
                let current = self.game(&id)?;
                let folders: Vec<Value> = list(&group["members"])
                    .iter()
                    .map(|a| a["folder"].clone())
                    .collect();
                let backup = self.dir.join("backups").join(&id).join(format!(
                    "{}-removed-{}",
                    now(),
                    uuid::Uuid::new_v4()
                ));
                self.stage("Backing up and removing addon…");
                let removed = inventory::run(
                    "remove",
                    text(&current["installation"], "root"),
                    &[
                        json!(folders).to_string(),
                        backup.to_string_lossy().into_owned(),
                    ],
                )?;
                self.scan(&id)?;
                self.state["packages"][&id] = json!(list(&self.state["packages"][&id])
                    .into_iter()
                    .filter(|p| !list(&p["folders"]).iter().any(|f| folders.contains(f)))
                    .collect::<Vec<_>>());
                self.persist()?;
                Ok(json!({"state":self.state,"removed":removed}))
            }
            "browse" | "choose-zip" => self.browse(method, args),
            _ => Err(format!("Unknown application command: {method}")),
        }
    }
    fn browse(&self, method: &str, args: &[Value]) -> Result<Value> {
        #[cfg(feature = "desktop")]
        {
            let dialog = rfd::FileDialog::new();
            let chosen = if method == "choose-zip" {
                dialog
                    .set_title("Choose an addon ZIP")
                    .add_filter("Addon ZIP", &["zip"])
                    .pick_file()
            } else if args.first().and_then(Value::as_str) == Some("file") {
                dialog.set_title("Select the WoW executable").pick_file()
            } else {
                dialog
                    .set_title("Select a WoW client or World of Warcraft folder")
                    .pick_folder()
            };
            Ok(chosen
                .map(|p| json!(p.to_string_lossy()))
                .unwrap_or(Value::Null))
        }
        #[cfg(not(feature = "desktop"))]
        {
            let _ = (method, args);
            Err("File dialogs require the desktop feature.".into())
        }
    }
}
pub fn shell_path(path: &str) -> String {
    if let Some(p) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{p}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("wow-service-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn preserves_damaged_inventory_and_loads_offline_snapshots() {
        let p = temp();
        fs::write(p.join("inventory.json"), "damaged").unwrap();
        let mut s = Service::new(p.clone(), std::sync::Arc::new(|_| {})).unwrap();
        assert!(s.dispatch("load", &[]).unwrap()["error"].is_string());
        assert!(s.dispatch("preferences", &[json!({})]).is_err());
        assert_eq!(
            fs::read_to_string(p.join("inventory.json")).unwrap(),
            "damaged"
        );
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn installed_zip_has_manifest_and_removal_backs_up_all_modules() {
        use std::io::Write;
        use zip::{write::SimpleFileOptions, ZipWriter};
        let p = temp();
        let root = p.join("_forever_");
        fs::create_dir_all(root.join("Interface/AddOns")).unwrap();
        let z = p.join("addon.zip");
        let mut zip = ZipWriter::new(fs::File::create(&z).unwrap());
        for (name, content) in [
            (
                "Example/Example.toc",
                "## Interface: 16001\n## Title: Example\n## Version: 1",
            ),
            (
                "Module/Module.toc",
                "## Interface: 16001\n## Title: Module\n## Version: 1",
            ),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let mut s = Service::new(p.join("data"), std::sync::Arc::new(|_| {})).unwrap();
        let state = s.dispatch("add", &[json!([root])]).unwrap();
        let id = text(&state, "selectedId").to_string();
        let plan = s.dispatch("inspect-zip", &[json!(id), json!(z)]).unwrap();
        s.dispatch("install-zip", &[plan["token"].clone()]).unwrap();
        assert_eq!(list(&s.state["packages"][&id][0]["folders"]).len(), 2);
        let folder = text(&s.state["packages"][&id][0], "primary").to_string();
        let out = s
            .dispatch("remove-addon", &[json!(id), json!(folder)])
            .unwrap();
        assert_eq!(list(&out["removed"]["removed"]).len(), 2);
        assert!(!root.join("Interface/AddOns/Example").exists());
        assert!(Path::new(text(&out["removed"], "backupPath"))
            .join("Example/Example.toc")
            .exists());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn lifecycle_preferences_migrate_and_survive_restart() {
        let p = temp();
        let mut old = defaults();
        old.as_object_mut().unwrap().remove("closeToTray");
        old["checkUpdatesOnStartup"] = json!("invalid");
        atomic(
            &p.join("inventory.json"),
            &serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let mut s = Service::new(
            p.clone(),
            std::sync::Arc::new(|_| panic!("Loading must stay offline")),
        )
        .unwrap();
        assert_eq!(s.state["closeToTray"], true);
        assert_eq!(s.state["checkUpdatesOnStartup"], true);
        s.dispatch(
            "preferences",
            &[json!({"closeToTray":false,"checkUpdatesOnStartup":false})],
        )
        .unwrap();
        s.dispatch("preferences", &[json!({"columns":["addon","folder"]})])
            .unwrap();
        let s = Service::new(p.clone(), std::sync::Arc::new(|_| {})).unwrap();
        assert_eq!(s.state["closeToTray"], false);
        assert_eq!(s.state["checkUpdatesOnStartup"], false);
        fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn cached_startup_and_column_preferences_survive_restart() {
        let p = temp();
        let mut state = defaults();
        state["games"] = json!([{"id":"offline","installation":{"root":"/missing/game"},"addons":[{"folder":"Example","name":"Example","version":"1"}]}]);
        atomic(
            &p.join("inventory.json"),
            &serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let mut s = Service::new(
            p.clone(),
            std::sync::Arc::new(|_| panic!("Startup must not scan or fetch.")),
        )
        .unwrap();
        assert_eq!(
            s.dispatch("load", &[]).unwrap()["state"]["games"][0]["addons"][0]["version"],
            "1"
        );
        s.dispatch("preferences", &[json!({"columns":["addon","folder"]})])
            .unwrap();
        let s = Service::new(p.clone(), std::sync::Arc::new(|_| {})).unwrap();
        assert_eq!(s.state["columns"], json!(["addon", "folder"]));
        fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn a_new_optional_folder_requires_fresh_confirmation() {
        use std::io::Write;
        use zip::{write::SimpleFileOptions, ZipWriter};
        let p = temp();
        let root = p.join("_forever_");
        fs::create_dir_all(root.join("Interface/AddOns")).unwrap();
        let archive = p.join("addon.zip");
        let mut zip = ZipWriter::new(fs::File::create(&archive).unwrap());
        for (name, toc) in [
            ("Main/Main.toc", "## Interface: 16001\n## Title: Main"),
            (
                "Optional/Optional.toc",
                "## Interface: 120105\n## Title: Optional\n## Dependencies: Main",
            ),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(toc.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let mut service = Service::new(p.join("data"), std::sync::Arc::new(|_| {})).unwrap();
        let state = service.dispatch("add", &[json!([root])]).unwrap();
        let id = text(&state, "selectedId").to_string();
        let plan = service
            .dispatch("inspect-zip", &[json!(id), json!(archive)])
            .unwrap();
        fs::create_dir(root.join("Interface/AddOns/Optional")).unwrap();
        fs::write(root.join("Interface/AddOns/Optional/original.lua"), "keep").unwrap();
        assert!(service
            .dispatch("install-zip", &[plan["token"].clone()])
            .unwrap_err()
            .contains("appeared after validation"));
        assert_eq!(
            fs::read_to_string(root.join("Interface/AddOns/Optional/original.lua")).unwrap(),
            "keep"
        );
        assert!(!root.join("Interface/AddOns/Main").exists());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn addon_list_roundtrip_pins_files_and_excludes_private_state() {
        let dir = std::env::temp_dir().join(format!("addon-list-{}", uuid::Uuid::new_v4()));
        let mut service = Service::new(dir.clone(), std::sync::Arc::new(|_| {})).unwrap();
        service.state["games"] = json!([{
            "id":"friend", "installation":{"root":"/private/game","flavor":"Forever"},
            "addons":[{"folder":"Example","name":"Example","version":"1"}]
        }]);
        service.state["packages"]["friend"] = json!([{
            "primary":"Example","folders":["Example"],"installedVersion":"1",
            "url":"https://www.curseforge.com/wow/addons/example","fileId":42
        }]);
        let document = service.export_list("friend").unwrap();
        assert_eq!(
            document["addons"][0]["url"],
            "https://www.curseforge.com/wow/addons/example/files/42"
        );
        assert!(!document.to_string().contains("/private"));
        assert_eq!(
            service.select_list("friend", &json!([0])).unwrap(),
            document
        );
        assert!(service.select_list("friend", &json!([])).is_err());
        assert!(service.select_list("friend", &json!([0, 0])).is_err());
        assert!(service.select_list("friend", &json!([1])).is_err());
        assert!(service.select_list("friend", &json!([-1])).is_err());
        service.state["games"][0]["addons"]
            .as_array_mut()
            .unwrap()
            .push(json!({"folder":"Other", "name":"Other", "version":"2"}));
        let subset = service.select_list("friend", &json!([0])).unwrap();
        assert_eq!(subset["addons"].as_array().unwrap().len(), 1);
        assert_eq!(subset["addons"][0]["name"], "Example");
        Service::validate_list(&document, "Forever").unwrap();
        assert!(Service::validate_list(&document, "Retail").is_err());
        assert!(service.install_list("missing", &document).is_err());
        service.state["games"][0]["installation"]["flavor"] = json!("Retail");
        assert!(service
            .install_list("friend", &document)
            .unwrap_err()
            .contains("different WoW client"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn addon_lists_reject_invalid_sources_versions_and_duplicates() {
        let mut document = json!({"format":"wow-addons-list","schemaVersion":1,"flavor":"Forever",
            "addons":[{"name":"Example","version":"1","url":null}]});
        Service::validate_list(&document, "Forever").unwrap();
        assert!(Service::validate_list(&document, "Custom").is_err());
        document["addons"][0]["url"] = json!("https://evil.test/addon.zip");
        assert!(Service::validate_list(&document, "Forever").is_err());
        document["addons"][0]["url"] = json!("https://www.curseforge.com/wow/addons/example");
        let duplicate = document["addons"][0].clone();
        document["addons"].as_array_mut().unwrap().push(duplicate);
        assert!(Service::validate_list(&document, "Forever").is_err());
        document["addons"] = json!([]);
        document["schemaVersion"] = json!(2);
        assert!(Service::validate_list(&document, "Forever").is_err());
    }
}
