use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
pub fn list(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
pub fn bundles() -> Vec<Vec<String>> {
    serde_json::from_str(include_str!("../desktop/bundles.json")).unwrap()
}
fn root(parent: &[usize], mut n: usize) -> usize {
    while parent[n] != n {
        n = parent[n];
    }
    n
}
fn canonical(value: &str) -> String {
    if let Ok(u) = url::Url::parse(value) {
        let p: Vec<_> = u.path().trim_matches('/').split('/').collect();
        if u.scheme() == "https"
            && u.host_str() == Some("www.curseforge.com")
            && p.len() == 3
            && p[0] == "wow"
            && p[1] == "addons"
        {
            return format!(
                "https://www.curseforge.com/wow/addons/{}",
                p[2].to_lowercase()
            );
        }
    }
    value.to_string()
}
pub fn groups(addons: &Value, bindings: &Value, packages: &Value) -> Vec<Value> {
    let a = list(addons);
    let mut parent: Vec<usize> = (0..a.len()).collect();
    let by: BTreeMap<_, _> = a
        .iter()
        .enumerate()
        .map(|(i, a)| (text(a, "folder").to_string(), i))
        .collect();
    let compatible = |x: usize, y: usize| {
        let l = text(&bindings[text(&a[x], "folder")], "url");
        let r = text(&bindings[text(&a[y], "folder")], "url");
        if !l.is_empty() && !r.is_empty() {
            return canonical(l) == canonical(r);
        }
        match (
            a[x]["curseProjectId"].as_u64(),
            a[y]["curseProjectId"].as_u64(),
        ) {
            (Some(x), Some(y)) => x == y,
            _ => true,
        }
    };
    let join = |parent: &mut Vec<usize>, x: usize, y: usize| {
        let rx = root(parent, x);
        let ry = root(parent, y);
        if rx == ry {
            return;
        }
        if (0..a.len()).filter(|&i| root(parent, i) == rx).all(|i| {
            (0..a.len())
                .filter(|&j| root(parent, j) == ry)
                .all(|j| compatible(i, j))
        }) {
            parent[ry] = rx;
        }
    };
    let known = bundles();
    for b in &known {
        if let Some(&main) = by.get(&b[0]) {
            for f in b {
                if let Some(&i) = by.get(f) {
                    join(&mut parent, main, i);
                }
            }
        }
    }
    for p in list(packages) {
        let fs = list(&p["folders"]);
        let primary = by.get(text(&p, "primary")).copied().or_else(|| {
            fs.iter()
                .find_map(|f| by.get(f.as_str().unwrap_or("")).copied())
        });
        if let Some(main) = primary {
            for f in fs {
                if let Some(&i) = by.get(f.as_str().unwrap_or("")) {
                    join(&mut parent, main, i);
                }
            }
        }
    }
    let mut ids = BTreeMap::new();
    let mut urls = BTreeMap::new();
    for (i, v) in a.iter().enumerate() {
        let url = text(&bindings[text(v, "folder")], "url");
        if !url.is_empty() {
            if let Some(&other) = urls.get(&canonical(url)) {
                join(&mut parent, other, i);
            } else {
                urls.insert(canonical(url), i);
            }
        }
        if let Some(id) = v["curseProjectId"].as_u64() {
            if let Some(&other) = ids.get(&id) {
                join(&mut parent, other, i);
            } else {
                ids.insert(id, i);
            }
        }
    }
    let mut grouped: BTreeMap<usize, Vec<Value>> = BTreeMap::new();
    for (i, v) in a.iter().enumerate() {
        grouped.entry(root(&parent, i)).or_default().push(v.clone());
    }
    grouped
        .into_values()
        .map(|mut members| {
            let manifest = list(packages).into_iter().find(|p| {
                members
                    .iter()
                    .any(|a| text(a, "folder") == text(p, "primary"))
            });
            let primary = manifest
                .as_ref()
                .map(|p| text(p, "primary").to_string())
                .filter(|f| members.iter().any(|a| text(a, "folder") == f))
                .or_else(|| {
                    known
                        .iter()
                        .find(|b| members.iter().any(|a| text(a, "folder") == b[0]))
                        .map(|b| b[0].clone())
                });
            members.sort_by(|a, b| {
                text(a, "version")
                    .is_empty()
                    .cmp(&text(b, "version").is_empty())
                    .then(text(a, "folder").len().cmp(&text(b, "folder").len()))
                    .then(text(a, "folder").cmp(text(b, "folder")))
            });
            let i = primary
                .as_ref()
                .and_then(|f| members.iter().position(|a| text(a, "folder") == f))
                .unwrap_or(0);
            let main = members.remove(i);
            members.sort_by_key(|a| text(a, "folder").to_lowercase());
            members.insert(0, main.clone());
            let mut result = main;
            result["members"] = json!(members);
            if let Some(m) = manifest {
                if !text(&m, "title").is_empty() {
                    result["name"] = m["title"].clone();
                }
            }
            result
        })
        .collect()
}
pub fn link(group: &Value, bindings: &Value) -> Option<String> {
    list(&group["members"])
        .iter()
        .map(|a| text(&bindings[text(a, "folder")], "url").to_string())
        .find(|s| !s.is_empty())
}
pub fn identity(group: &Value, bindings: &Value) -> String {
    let members = list(&group["members"]);
    let mut linked = Vec::new();
    let mut ids = BTreeSet::new();
    let mut verified = BTreeSet::new();
    for a in &members {
        let b = &bindings[text(a, "folder")];
        if b["verified"] != true && !text(b, "url").is_empty() {
            linked.push(text(b, "url").to_string());
        }
        if let Some(id) = a["curseProjectId"].as_u64() {
            ids.insert(id.to_string());
        }
        if b["verified"] == true {
            if let Some(id) = b["projectId"].as_u64() {
                verified.insert(id.to_string());
            }
        }
    }
    linked.sort();
    let token = if !linked.is_empty() {
        format!("url:{}", linked.join("|"))
    } else if !ids.is_empty() {
        format!("cf:{}", ids.into_iter().collect::<Vec<_>>().join(","))
    } else if !verified.is_empty() {
        format!("cf:{}", verified.into_iter().collect::<Vec<_>>().join(","))
    } else {
        format!("folder:{}", text(group, "folder"))
    };
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
pub fn record(
    existing: &Value,
    addons: &Value,
    folders: &Value,
    digest: &str,
    source: &Value,
) -> Value {
    let names: BTreeSet<String> = list(folders)
        .iter()
        .filter_map(|f| f.as_str().map(str::to_string))
        .collect();
    let members: Vec<Value> = list(addons)
        .into_iter()
        .filter(|a| names.contains(text(a, "folder")))
        .collect();
    let grouped = groups(&json!(members), &json!({}), &json!([]));
    let main = members
        .iter()
        .find(|a| !source.is_null() && a["curseProjectId"] == source["id"])
        .cloned()
        .or_else(|| {
            grouped.into_iter().max_by_key(|g| {
                (
                    list(&g["members"]).len(),
                    members
                        .iter()
                        .filter(|a| list(&a["dependencies"]).contains(&g["folder"]))
                        .count(),
                )
            })
        });
    let Some(main) = main else {
        return existing.clone();
    };
    let id = if source.is_null() {
        format!("folder:{}", text(&main, "folder"))
    } else {
        format!("curseforge:{}", source["id"])
    };
    let mut entry = json!({
        "id": id,
        "primary": main["folder"],
        "installedVersion": main["version"],
        "folders": folders,
        "digest": digest,
    });
    if !source.is_null() {
        entry["title"] = source["name"].clone();
        entry["projectId"] = source["id"].clone();
        entry["fileId"] = source["fileId"].clone();
        entry["url"] = source["url"].clone();
    }
    let mut retained = Vec::new();
    for mut p in list(existing) {
        if p["id"] == entry["id"] {
            continue;
        }
        p["folders"] = json!(list(&p["folders"])
            .into_iter()
            .filter(|f| !names.contains(f.as_str().unwrap_or("")))
            .collect::<Vec<_>>());
        if !list(&p["folders"]).is_empty() {
            retained.push(p);
        }
    }
    retained.push(entry);
    json!(retained)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_bundles_and_manifests_preserve_ownership() {
        let a = json!([{"folder":"Bagnon","version":"1"},{"folder":"BagBrother"},{"folder":"Unrelated","dependencies":["Bagnon"]}]);
        let g = groups(&a, &json!({}), &json!([]));
        assert_eq!(g.len(), 2);
        assert_eq!(
            list(&g.iter().find(|g| g["folder"] == "Bagnon").unwrap()["members"]).len(),
            2
        );
        let g = groups(
            &a,
            &json!({}),
            &json!([{"primary":"Bagnon","folders":["Bagnon","BagBrother","Unrelated"]}]),
        );
        assert_eq!(g.len(), 1);
    }
    #[test]
    fn conflicting_links_keep_addons_separate() {
        let a = json!([{"folder":"Bagnon","curseProjectId":1},{"folder":"BagBrother","curseProjectId":1}]);
        assert_eq!(
            groups(
                &a,
                &json!({
                    "Bagnon": {
                        "url": "https://www.curseforge.com/wow/addons/one",
                    },
                    "BagBrother": {
                        "url": "https://www.curseforge.com/wow/addons/two",
                    },
                }),
                &json!([])
            )
            .len(),
            2
        );
    }
}
