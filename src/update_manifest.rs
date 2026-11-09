use serde_json::Value;
use std::time::Duration;

const RELEASE_API: &str = "https://api.github.com/repos/h2u/wow-addons/releases/latest";
const ASSET_API: &str = "https://api.github.com/repos/h2u/wow-addons/releases/assets/";

fn manifest_url(release: &Value) -> Result<String, String> {
    if release["draft"] != false || release["prerelease"] != false {
        return Err("Expected a published stable release.".into());
    }
    let asset = release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|asset| asset["name"] == "latest.json"))
        .ok_or("The latest release's update manifest is not available yet.")?;
    // Construct from the ID rather than trusting an arbitrary metadata URL.
    let id = asset["id"]
        .as_u64()
        .ok_or("Invalid update manifest asset ID.")?;
    Ok(format!("{ASSET_API}{id}"))
}

pub async fn fetch() -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("wow-addons/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let release: Value = client
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    client
        .get(manifest_url(&release)?)
        .header("Accept", "application/octet-stream")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selects_manifest_via_api_instead_of_download_redirect() {
        let release = json!({"draft":false,"prerelease":false,"assets":[
            {"id":1,"name":"app.exe"},
            {"id":42,"name":"latest.json","url":"https://untrusted.example"}
        ]});
        assert_eq!(manifest_url(&release).unwrap(), format!("{ASSET_API}42"));
    }

    #[test]
    fn incomplete_or_unstable_releases_are_errors_not_up_to_date() {
        for release in [
            json!({"draft":false,"prerelease":false,"assets":[]}),
            json!({"draft":true,"prerelease":false,"assets":[{"id":42,"name":"latest.json"}]}),
            json!({"draft":false,"prerelease":true,"assets":[{"id":42,"name":"latest.json"}]}),
            json!({"draft":false,"prerelease":false,"assets":[{"id":"bad","name":"latest.json"}]}),
        ] {
            assert!(manifest_url(&release).is_err());
        }
    }
}
