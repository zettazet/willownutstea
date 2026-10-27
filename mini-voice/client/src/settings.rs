use serde::{Deserialize, Serialize};
use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub server: String,
    pub token: Vec<u8>,
}
fn path() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
        .join("MiniVoice")
        .join("settings.json")
}
fn crypt(data: &[u8], protect: bool) -> Option<Vec<u8>> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return None;
        }
        let result = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        Some(result)
    }
}
impl Settings {
    pub fn load() -> Self {
        std::fs::read(path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn token(&self) -> Option<String> {
        String::from_utf8(crypt(&self.token, false)?).ok()
    }
    pub fn save(&mut self, server: String, token: Option<&str>) -> anyhow::Result<()> {
        self.server = server;
        self.token = match token {
            Some(t) => crypt(t.as_bytes(), true)
                .ok_or_else(|| anyhow::anyhow!("Windows could not protect session"))?,
            None => vec![],
        };
        let p = path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let temp = p.with_extension("tmp");
        std::fs::write(&temp, serde_json::to_vec(self)?)?;
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
        std::fs::rename(temp, p)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_session_protection_roundtrip() {
        let token = b"test token; not a real credential";
        let protected = crypt(token, true).expect("DPAPI protection");
        assert_ne!(protected, token);
        assert_eq!(crypt(&protected, false).expect("DPAPI unprotect"), token);
        assert!(crypt(b"not an encrypted token", false).is_none());
    }
}
