use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub trigger: u32,
    pub delay_ms: u32,
    pub gap: i32,
    pub preview: bool,
    pub first_run: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            trigger: 0xA3,
            delay_ms: 180,
            gap: 8,
            preview: true,
            first_run: true,
        }
    }
}
impl Config {
    pub fn normalize(&mut self) {
        if ![0xA3, 0x77, 0x78].contains(&self.trigger) {
            self.trigger = 0xA3;
        }
        self.delay_ms = self.delay_ms.clamp(80, 600);
        self.gap = self.gap.clamp(0, 32);
    }
    pub fn folder() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Orbit")
    }
    pub fn load() -> Result<Self, String> {
        let path = Self::folder().join("settings.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read(path).map_err(|e| e.to_string())?;
        let mut config: Self = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        config.normalize();
        Ok(config)
    }
    pub fn save(&self) -> Result<(), String> {
        let folder = Self::folder();
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let tmp = folder.join("settings.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        // Windows rename cannot replace an existing file. Keep the old config on write failure.
        #[cfg(windows)]
        unsafe {
            use std::os::windows::ffi::OsStrExt;
            use windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
            let src: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
            let dst: Vec<u16> = folder
                .join("settings.json")
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            MoveFileExW(
                PCWSTR(src.as_ptr()),
                PCWSTR(dst.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|e| e.to_string())?;
        }
        #[cfg(not(windows))]
        fs::rename(tmp, folder.join("settings.json")).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupted_preferences_cannot_create_invalid_geometry_or_reserved_trigger() {
        let mut c: Config =
            serde_json::from_str(r#"{"trigger":91,"gap":-900,"delay_ms":0}"#).unwrap();
        c.normalize();
        assert_eq!(c.trigger, 0xA3);
        assert_eq!(c.gap, 0);
        assert_eq!(c.delay_ms, 80);
        assert!(c.preview);
    }
}
