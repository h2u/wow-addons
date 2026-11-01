// Read the previous Windows encrypted setting once. No previous runtime is needed.
#[cfg(windows)]
pub fn legacy_key(dir: &std::path::Path) -> Option<String> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::Value;
    let saved: Value =
        serde_json::from_slice(&std::fs::read(dir.join("credentials.json")).ok()?).ok()?;
    let encrypted = STANDARD.decode(saved["curseforge"].as_str()?).ok()?;
    if !encrypted.starts_with(b"v10") || encrypted.len() < 31 {
        return None;
    }
    let state: Value =
        serde_json::from_slice(&std::fs::read(dir.join("Local State")).ok()?).ok()?;
    let protected = STANDARD
        .decode(state["os_crypt"]["encrypted_key"].as_str()?)
        .ok()?;
    let mut key = unprotect(protected.strip_prefix(b"DPAPI")?)?;
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
    key.fill(0);
    let plain = cipher
        .decrypt(Nonce::from_slice(&encrypted[3..15]), &encrypted[15..])
        .ok()?;
    let result = String::from_utf8(plain).ok()?;
    if result.trim().is_empty() || result.len() > 512 {
        None
    } else {
        Some(result)
    }
}
#[cfg(windows)]
fn unprotect(bytes: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().ok()?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI allocates output; copy it, erase that allocation, and release it.
    unsafe {
        if CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        ) == 0
        {
            return None;
        }
        let plain = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = plain.to_vec();
        plain.fill(0);
        LocalFree(output.pbData as *mut core::ffi::c_void);
        Some(result)
    }
}
