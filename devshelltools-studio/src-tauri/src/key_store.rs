use crate::error::{DstError, DstResult};
use crate::workspace;
use crate::workspace_path::validate_component;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"DST1";

pub fn protect(secret: &[u8]) -> DstResult<Vec<u8>> {
    dpapi_protect(secret)
}

pub fn unprotect(ciphertext: &[u8]) -> DstResult<Vec<u8>> {
    dpapi_unprotect(ciphertext)
}

pub fn load_secret(profile_id: &str) -> DstResult<String> {
    validate_component(profile_id, "AI Profile ID")?;
    let encrypted_path = encrypted_path(profile_id);
    let legacy_path = legacy_path(profile_id)?;

    if encrypted_path.exists() {
        match read_encrypted(&encrypted_path) {
            Ok(secret) => return Ok(secret),
            Err(encrypted_error) if !legacy_path.exists() => return Err(encrypted_error),
            Err(encrypted_error) => {
                log::warn!("读取加密 Key 失败，尝试旧明文兼容：{encrypted_error}");
            }
        }
    }

    if legacy_path.exists() {
        let legacy = std::fs::read_to_string(&legacy_path)?.trim().to_string();
        if legacy.is_empty() {
            return Err(DstError::Other("API Key 为空".into()));
        }
        if let Err(error) = save_secret(profile_id, &legacy) {
            log::warn!("旧 API Key 加密迁移失败，继续使用明文兼容：{error}");
        }
        return Ok(legacy);
    }

    Err(DstError::Other("未配置 API Key".into()))
}

pub fn save_secret(profile_id: &str, secret: &str) -> DstResult<()> {
    validate_component(profile_id, "AI Profile ID")?;
    let target = encrypted_path(profile_id);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let protected = protect(secret.as_bytes())?;
    let mut payload = Vec::with_capacity(MAGIC.len() + protected.len());
    payload.extend_from_slice(MAGIC);
    payload.extend_from_slice(&protected);
    write_atomic(&target, &payload)?;
    let verified = read_encrypted(&target)?;
    if verified != secret {
        return Err(DstError::Other("加密 Key 写入校验失败".into()));
    }
    let legacy = legacy_path(profile_id)?;
    if legacy.exists() {
        std::fs::remove_file(legacy)?;
    }
    Ok(())
}

pub fn has_secret(profile_id: &str) -> bool {
    validate_component(profile_id, "AI Profile ID").is_ok()
        && (encrypted_path(profile_id).exists()
            || legacy_path(profile_id).is_ok_and(|path| path.exists()))
}

pub fn delete_secret(profile_id: &str) -> DstResult<()> {
    validate_component(profile_id, "AI Profile ID")?;
    let encrypted = encrypted_path(profile_id);
    let legacy = legacy_path(profile_id)?;
    if encrypted.exists() {
        std::fs::remove_file(encrypted)?;
    }
    if legacy.exists() {
        std::fs::remove_file(legacy)?;
    }
    Ok(())
}

pub fn mask_secret_for_display(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        return "****".into();
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}...{suffix}")
}

fn encrypted_path(profile_id: &str) -> PathBuf {
    workspace::studio_dir()
        .join("ai_keys")
        .join(format!("{profile_id}.dpapi"))
}

fn legacy_path(profile_id: &str) -> DstResult<PathBuf> {
    validate_component(profile_id, "AI Profile ID")?;
    Ok(workspace::studio_dir()
        .join("ai_keys")
        .join(format!("{profile_id}.txt")))
}

fn read_encrypted(path: &Path) -> DstResult<String> {
    let payload = std::fs::read(path)?;
    let ciphertext = payload
        .strip_prefix(MAGIC)
        .ok_or_else(|| DstError::Other("API Key 文件格式不受支持".into()))?;
    let plain = unprotect(ciphertext)?;
    String::from_utf8(plain).map_err(|_| DstError::Other("API Key 不是有效 UTF-8".into()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> DstResult<()> {
    let tmp = path.with_extension(format!(
        "dpapi.tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    let backup = path.with_extension("dpapi.bak");
    if path.exists() {
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(path, &backup)?;
    }
    if let Err(error) = std::fs::rename(&tmp, path) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, path);
        }
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

#[cfg(windows)]
fn dpapi_protect(secret: &[u8]) -> DstResult<Vec<u8>> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let size = u32::try_from(secret.len())
        .map_err(|_| DstError::Other("API Key 过长，无法加密".into()))?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: size,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let result = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let protected = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(protected)
}

#[cfg(windows)]
fn dpapi_unprotect(ciphertext: &[u8]) -> DstResult<Vec<u8>> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let size = u32::try_from(ciphertext.len())
        .map_err(|_| DstError::Other("API Key 密文过长".into()))?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: size,
        pbData: ciphertext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let result = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let plain = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(plain)
}

#[cfg(not(windows))]
fn dpapi_protect(_secret: &[u8]) -> DstResult<Vec<u8>> {
    Err(DstError::Other("当前平台不支持 Windows DPAPI".into()))
}

#[cfg(not(windows))]
fn dpapi_unprotect(_ciphertext: &[u8]) -> DstResult<Vec<u8>> {
    Err(DstError::Other("当前平台不支持 Windows DPAPI".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn protected_value_roundtrips_for_current_user() {
        let secret = b"sk-test-value";
        let protected = protect(secret).expect("protect");
        assert_ne!(protected, secret);
        assert_eq!(unprotect(&protected).expect("unprotect"), secret);
    }

    #[test]
    fn legacy_plaintext_is_readable_before_migration() {
        with_temp_docs(|| {
            let path = legacy_path("profile").expect("legacy path");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("keys dir");
            std::fs::write(&path, "legacy-secret").expect("legacy key");
            assert_eq!(load_secret("profile").expect("load"), "legacy-secret");
        });
    }

    #[test]
    fn traversal_profile_id_is_rejected_before_file_io() {
        assert!(load_secret("../../outside").is_err());
        assert!(save_secret("../../outside", "secret").is_err());
    }

    #[test]
    fn mask_uses_unicode_boundaries() {
        assert_eq!(
            mask_secret_for_display("密钥密钥密钥密钥密"),
            "密钥密钥...钥密钥密"
        );
        assert_eq!(mask_secret_for_display("short"), "****");
    }

    fn with_temp_docs(f: impl FnOnce()) {
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!(
            "dst-key-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temp root");
        let old = std::env::var_os("DST_MY_DOCUMENTS");
        std::env::set_var("DST_MY_DOCUMENTS", &root);
        f();
        if let Some(old) = old {
            std::env::set_var("DST_MY_DOCUMENTS", old);
        } else {
            std::env::remove_var("DST_MY_DOCUMENTS");
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
