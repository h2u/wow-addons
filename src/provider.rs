use crate::grouping::{list, text};
use md5::Md5;
use reqwest::blocking::{Client, Response};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
pub type Result<T> = std::result::Result<T, String>;
pub fn parse_url(input: &str) -> Result<(String, Option<u64>, String)> {
    let u = url::Url::parse(input.trim()).map_err(|_| "Enter a CurseForge addon URL.")?;
    if u.scheme() != "https"
        || !matches!(u.host_str(), Some("www.curseforge.com" | "curseforge.com"))
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        return Err("Use an HTTPS CurseForge addon URL.".into());
    }
    let p: Vec<_> = u.path().trim_matches('/').split('/').collect();
    if !matches!(p.len(), 3 | 5)
        || p[0] != "wow"
        || p[1] != "addons"
        || p[2].is_empty()
        || !p[2].chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Use a CurseForge project or file URL.".into());
    }
    let id = if p.len() == 5 {
        if !matches!(p[3], "files" | "download") {
            return Err("Invalid file URL.".into());
        }
        Some(p[4].parse::<u64>().map_err(|_| "Invalid file ID.")?)
    } else {
        None
    };
    let slug = p[2].to_lowercase();
    Ok((
        slug.clone(),
        id,
        format!("https://www.curseforge.com/wow/addons/{slug}"),
    ))
}
pub fn type_id(flavor: &str) -> Result<u64> {
    match flavor {
        "Forever" => Ok(88568),
        "Classic Era" => Ok(67408),
        f if f.starts_with("Retail") => Ok(517),
        _ => Err(
            "Automatic release selection for this client is not configured. \
             Import a ZIP built for this client."
                .into(),
        ),
    }
}
pub fn supports(file: &Value, project: &Value, flavor: &str) -> bool {
    let Ok(id) = type_id(flavor) else {
        return false;
    };
    list(&file["sortableGameVersions"])
        .iter()
        .any(|v| v["gameVersionTypeId"] == id)
        || list(&project["latestFilesIndexes"])
            .iter()
            .any(|v| v["fileId"] == file["id"] && v["gameVersionTypeId"] == id)
}
pub struct Provider {
    pub client: Client,
}
impl Provider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(60))
                .user_agent("WoW-Addons/0.1")
                .build()
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn request(&self, key: &str, route: &str, body: Option<&Value>) -> Result<Value> {
        if key.trim().is_empty() {
            return Err("Configure your CurseForge API key in Settings first.".into());
        }
        let url = format!("https://api.curseforge.com/v1/{route}");
        let builder = if let Some(body) = body {
            self.client.post(url).json(body)
        } else {
            self.client.get(url)
        };
        let r = builder
            .header("x-api-key", key)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| format!("CurseForge request failed: {e}"))?;
        if !r.status().is_success() {
            return Err(format!(
                "CurseForge API returned HTTP {}",
                r.status().as_u16()
            ));
        }
        if r.content_length().unwrap_or(0) > 16 * 1024 * 1024 {
            return Err("API response is too large.".into());
        }
        let mut bytes = Vec::new();
        r.take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("API response is too large.".into());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid API response.")?;
        Ok(value["data"].clone())
    }
    pub fn project(&self, url: &str, key: &str) -> Result<Value> {
        let (slug, _, _) = parse_url(url)?;
        let data = self.request(key, &format!("mods/search?gameId=1&slug={slug}"), None)?;
        list(&data)
            .into_iter()
            .find(|p| p["gameId"] == 1 && text(p, "slug").eq_ignore_ascii_case(&slug))
            .ok_or("CurseForge project was not found.".into())
    }
    pub fn latest(&self, p: &Value, flavor: &str, key: &str) -> Result<Value> {
        let id = type_id(flavor)?;
        let index = list(&p["latestFilesIndexes"])
            .into_iter()
            .filter(|v| v["gameVersionTypeId"] == id && v["releaseType"] == 1)
            .max_by_key(|v| v["fileId"].as_u64().unwrap_or(0));
        let file = if let Some(index) = index {
            let file_id = index["fileId"].as_u64().unwrap_or(0);
            list(&p["latestFiles"])
                .into_iter()
                .find(|f| f["id"] == file_id)
                .map(Ok)
                .unwrap_or_else(|| {
                    self.request(key, &format!("mods/{}/files/{file_id}", p["id"]), None)
                })?
        } else {
            let records = self.request(
                key,
                &format!("mods/{}/files?gameVersionTypeId={id}&pageSize=50", p["id"]),
                None,
            )?;
            list(&records)
                .into_iter()
                .filter(|f| {
                    f["releaseType"] == 1 && f["isAvailable"] != false && supports(f, p, flavor)
                })
                .max_by_key(date)
                .ok_or(format!("No compatible stable release for {flavor}."))?
        };
        if file["releaseType"] != 1 || file["isAvailable"] == false || !supports(&file, p, flavor) {
            return Err(format!("No compatible stable release for {flavor}."));
        }
        Ok(file)
    }
    pub fn selection(&self, url: &str, flavor: &str, key: &str) -> Result<Value> {
        let (_, id, base) = parse_url(url)?;
        let p = self.project(url, key)?;
        let f = if let Some(id) = id {
            self.request(key, &format!("mods/{}/files/{id}", p["id"]), None)?
        } else {
            self.latest(&p, flavor, key)?
        };
        if f["modId"]
            .as_u64()
            .is_some_and(|id| Some(id) != p["id"].as_u64())
            || f["isAvailable"] == false
            || !supports(&f, &p, flavor)
        {
            return Err(format!("This file is not available for {flavor}."));
        }
        let latest_id = list(&p["latestFilesIndexes"])
            .iter()
            .filter(|i| {
                i["releaseType"] == 1 && i["gameVersionTypeId"] == type_id(flavor).unwrap_or(0)
            })
            .filter_map(|i| i["fileId"].as_u64())
            .max();
        Ok(json!({
            "source": {
                "id": p["id"],
                "slug": p["slug"],
                "name": p["name"],
                "url": base,
                "fileId": f["id"],
                "fileVersion": f["displayName"],
                "isLatestStable": f["releaseType"]==1&&(id.is_none()||f["id"].as_u64()==latest_id),
            },
            "downloadPage": format!("{base}/download/{}",f["id"]),
            "project": p,
            "file": f,
        }))
    }
    pub fn releases(&self, url: &str, flavor: &str, key: &str, index: u64) -> Result<Value> {
        if index > 9950 || !index.is_multiple_of(50) {
            return Err("Invalid release page.".into());
        }
        let p = self.project(url, key)?;
        let (_, _, base) = parse_url(url)?;
        let rows = self.request(
            key,
            &format!(
                "mods/{}/files?gameVersionTypeId={}&index={index}&pageSize=50",
                p["id"],
                type_id(flavor)?
            ),
            None,
        )?;
        let records = list(&rows);
        let next = if records.len() == 50 && index < 9950 {
            Some(index + 50)
        } else {
            None
        };
        let mut files: Vec<_> = records
            .into_iter()
            .filter(|f| f["isAvailable"] != false && supports(f, &p, flavor))
            .collect();
        files.sort_by_key(|f| std::cmp::Reverse(date(f)));
        let releases: Vec<_> = files
            .iter()
            .map(|file| {
                let channel = match file["releaseType"].as_u64() {
                    Some(1) => "Stable",
                    Some(2) => "Beta",
                    Some(3) => "Alpha",
                    _ => "Other",
                };
                json!({
                    "id": file["id"],
                    "name": file.get("displayName").unwrap_or(&file["fileName"]),
                    "date": file["fileDate"],
                    "channel": channel,
                    "gameVersions": file["gameVersions"],
                    "url": format!("{base}/files/{}", file["id"]),
                })
            })
            .collect();
        Ok(json!({
            "name": p["name"],
            "url": base,
            "files": releases,
            "nextIndex": next,
        }))
    }
    pub fn response(&self, url: &str, icon: bool) -> Result<Response> {
        let mut target = url::Url::parse(url).map_err(|e| e.to_string())?;
        for _ in 0..6 {
            let host = target.host_str().unwrap_or("");
            let allowed =
                host.ends_with(".forgecdn.net") || (!icon && host == "www.curseforge.com");
            if target.scheme() != "https"
                || !allowed
                || !target.username().is_empty()
                || target.password().is_some()
                || target.port().is_some()
            {
                return Err("Download points to an unsupported host.".into());
            }
            let r = self
                .client
                .get(target.clone())
                .send()
                .map_err(|e| e.to_string())?;
            if r.status().is_redirection() {
                let location = r
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or("Download redirect has no location.")?;
                target = target.join(location).map_err(|e| e.to_string())?;
                continue;
            }
            return Ok(r);
        }
        Err("Too many download redirects.".into())
    }
    pub fn download(
        &self,
        selection: &Value,
        dir: &Path,
        progress: &dyn Fn(String),
    ) -> Result<Option<PathBuf>> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = &selection["file"];
        let project = &selection["project"];
        let url = if text(file, "downloadUrl").is_empty() {
            format!(
                "https://www.curseforge.com/api/v1/mods/{}/files/{}/download",
                project["id"], file["id"]
            )
        } else {
            text(file, "downloadUrl").to_string()
        };
        let mut response = self.response(&url, false)?;
        if !response.status().is_success()
            || response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .contains("text/html")
        {
            return Ok(None);
        }
        let expected = file["fileLength"].as_u64().unwrap_or(0);
        let limit = 256 * 1024 * 1024;
        if expected > limit || response.content_length().unwrap_or(0) > limit {
            return Err("Archive exceeds 256 MB.".into());
        }
        let temp = dir.join(format!(".download-{}.zip", uuid::Uuid::new_v4()));
        let outcome = (|| {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|e| e.to_string())?;
            let (mut sha256, mut sha1, mut md5) = (Sha256::new(), Sha1::new(), Md5::new());
            let mut count = 0u64;
            let mut header = Vec::new();
            let mut buffer = [0u8; 65536];
            let mut reported = 0u64;
            loop {
                let n = response.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                count += n as u64;
                if count > limit {
                    return Err("Archive exceeds 256 MB.".into());
                }
                if header.len() < 4 {
                    header.extend_from_slice(&buffer[..n.min(4 - header.len())]);
                }
                let part = &buffer[..n];
                sha256.update(part);
                sha1.update(part);
                md5.update(part);
                output.write_all(part).map_err(|e| e.to_string())?;
                if count - reported >= 256 * 1024 {
                    progress(format!(
                        "Downloading ZIP: {} KB / {} KB",
                        count / 1024,
                        expected / 1024
                    ));
                    reported = count;
                }
            }
            output.sync_all().map_err(|e| e.to_string())?;
            drop(output);
            if !header.starts_with(b"PK") {
                return Ok(None);
            }
            if expected != 0 && expected != count {
                return Err("Downloaded size does not match release metadata.".into());
            }
            let hash1 = format!("{:x}", sha1.finalize());
            let hash2 = format!("{:x}", md5.finalize());
            for h in list(&file["hashes"]) {
                let got = match h["algo"].as_u64() {
                    Some(1) => &hash1,
                    Some(2) => &hash2,
                    _ => continue,
                };
                if !got.eq_ignore_ascii_case(text(&h, "value")) {
                    return Err("Downloaded hash does not match release metadata.".into());
                }
            }
            let digest = format!("{:x}", sha256.finalize());
            let archive = dir.join(format!("{digest}.zip"));
            if !archive.exists() {
                fs::rename(&temp, &archive).map_err(|e| e.to_string())?;
            }
            Ok(Some(archive))
        })();
        let _ = fs::remove_file(temp);
        outcome
    }
}
pub fn date(f: &Value) -> i64 {
    chrono::DateTime::parse_from_rfc3339(text(f, "fileDate"))
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}
fn normalize(s: &str) -> String {
    let s = s
        .trim()
        .trim_start_matches('#')
        .trim_end_matches(".zip")
        .to_lowercase();
    let s = if s.starts_with('v') && s.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
        s[1..].to_string()
    } else {
        s
    };
    regex::Regex::new("-(release|stable|forever|retail|classic)$")
        .unwrap()
        .replace(&s, "")
        .to_string()
}
pub fn assess(
    version: &str,
    id: Option<u64>,
    latest: &Value,
    installed: &Value,
    hashes: &Value,
) -> Value {
    if id.is_some() && id == latest["id"].as_u64() {
        return json!({
            "status": "current",
            "reason": "Installed file matches the latest compatible release.",
        });
    }
    if list(&latest["hashes"]).iter().any(|h| {
        let algo = match h["algo"].as_u64() {
            Some(1) => "sha1",
            Some(2) => "md5",
            _ => return false,
        };
        !text(hashes, algo).is_empty() && text(hashes, algo).eq_ignore_ascii_case(text(h, "value"))
    }) {
        return json!({
            "status": "current",
            "reason": "Imported ZIP hash matches the latest release.",
        });
    }
    if id.is_some() && !installed.is_null() && date(installed) > 0 && date(latest) > 0 {
        return if date(installed) > date(latest) {
            json!({
                "status": "ahead",
                "reason": "Installed build is newer than stable.",
            })
        } else {
            json!({
                "status": "available",
                "reason": "A newer compatible release is available.",
            })
        };
    }
    let a = normalize(version);
    let b = normalize(text(latest, "displayName"));
    if !a.is_empty() && a == b {
        return json!({
            "status": "current",
            "reason": "Installed version matches the release label.",
        });
    }
    let numeric = |s: &str| -> Option<Vec<u64>> {
        let parts: Vec<_> = s.split('.').collect();
        if !(2..=4).contains(&parts.len()) {
            return None;
        }
        parts.into_iter().map(|v| v.parse().ok()).collect()
    };
    if let (Some(mut x), Some(mut y)) = (numeric(&a), numeric(&b)) {
        let n = x.len().max(y.len());
        x.resize(n, 0);
        y.resize(n, 0);
        return match x.cmp(&y) {
            std::cmp::Ordering::Less => {
                json!({"status":"available","reason":"A newer version is available."})
            }
            std::cmp::Ordering::Greater => {
                json!({
                    "status": "ahead",
                    "reason": "Installed version is ahead of stable.",
                })
            }
            _ => json!({"status":"current","reason":"Installed version matches stable."}),
        };
    }
    json!({
        "status": "unverified",
        "reason": "The installed release file is unknown and version labels cannot be compared reliably.",
    })
}
pub fn hashes(path: &Path) -> Result<Value> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let (mut a, mut b, mut c) = (Sha256::new(), Sha1::new(), Md5::new());
    let mut buffer = [0u8; 65536];
    let mut count = 0;
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n;
        if count > 256 * 1024 * 1024 {
            return Err("ZIP exceeds 256 MB.".into());
        }
        a.update(&buffer[..n]);
        b.update(&buffer[..n]);
        c.update(&buffer[..n]);
    }
    Ok(json!({
        "sha256": format!("{:x}",a.finalize()),
        "sha1": format!("{:x}",b.finalize()),
        "md5": format!("{:x}",c.finalize()),
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_only_curseforge_links() {
        assert_eq!(
            parse_url("https://www.curseforge.com/wow/addons/bagnon/files/42")
                .unwrap()
                .1,
            Some(42)
        );
        assert!(parse_url("https://evil.test/wow/addons/bagnon").is_err());
        assert!(parse_url("https://www.curseforge.com@evil.test/wow/addons/bagnon").is_err());
    }
    #[test]
    fn version_labels_and_file_ids_are_distinct() {
        let latest = json!({"id":42,"displayName":"1.2.3"});
        assert_eq!(
            assess("1.2.2", None, &latest, &Value::Null, &Value::Null)["status"],
            "available"
        );
        assert_eq!(
            assess("Forever 1.60.1", None, &latest, &Value::Null, &Value::Null)["status"],
            "unverified"
        );
        assert_eq!(
            assess("", Some(42), &latest, &Value::Null, &Value::Null)["status"],
            "current"
        );
    }
    #[test]
    fn game_flavors_do_not_cross() {
        let f = json!({"sortableGameVersions":[{"gameVersionTypeId":517}]});
        assert!(!supports(&f, &Value::Null, "Forever"));
        assert!(supports(&f, &Value::Null, "Retail"));
    }
}
