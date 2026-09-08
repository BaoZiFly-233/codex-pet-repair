use crate::native::wide;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{Foundation::*, System::Registry::*};

pub fn data_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("data")
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub automatic: bool,
    pub tray_only: bool,
    pub look_at_mouse: bool,
    pub verified_versions: Vec<String>,
}
impl Settings {
    pub fn confirm_version(&mut self, version: &str) {
        if !self.verified_versions.iter().any(|v| v == version) {
            self.verified_versions.push(version.into());
        }
    }
    pub fn load() -> Self {
        fs::read(data_dir().join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        write_json(&data_dir().join("settings.json"), self)
    }
}
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    let mut f = fs::File::create(&temp).map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    // Windows rename does not replace existing files; use MoveFileExW atomically.
    let a = wide(&temp.to_string_lossy());
    let b = wide(&path.to_string_lossy());
    unsafe extern "system" {
        fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
    }
    if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(crate::native::error("MoveFileEx"));
    }
    Ok(())
}
pub fn log(message: &str) {
    event("APP_DIAGNOSTIC", None, "程序诊断", message);
}
pub fn event(code: &str, operation: Option<&str>, message: &str, detail: &str) {
    static LOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let Ok(_lock) = LOG_LOCK.lock() else {
        return;
    };
    let root = data_dir();
    let _ = fs::create_dir_all(&root);
    let p = root.join("events.log");
    if fs::metadata(&p).is_ok_and(|m| m.len() > 1_000_000) {
        let _ = fs::remove_file(root.join("events.old.log"));
        let _ = fs::rename(&p, root.join("events.old.log"));
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(
            f,
            "{}  [{}] {}\n事件编号：{}{}\n{}\n",
            timestamp(),
            if code.contains("FAILED") {
                "错误"
            } else {
                "记录"
            },
            message,
            code,
            operation
                .map(|id| format!(" · 修复编号：{id}"))
                .unwrap_or_default(),
            detail
        );
    }
}
pub fn timestamp() -> String {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    unsafe extern "system" {
        fn GetLocalTime(time: *mut SYSTEMTIME);
    }
    let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe {
        GetLocalTime(&mut t);
    }
    let mut zone: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let zone_id = unsafe { GetTimeZoneInformation(&mut zone) };
    let offset = -(zone.Bias
        + if zone_id == 2 {
            zone.DaylightBias
        } else {
            zone.StandardBias
        });
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {}{:02}:{:02}",
        t.wYear,
        t.wMonth,
        t.wDay,
        t.wHour,
        t.wMinute,
        t.wSecond,
        if offset < 0 { "-" } else { "+" },
        offset.abs() / 60,
        offset.abs() % 60
    )
}
const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const NAME: &str = "CodexPetRepair";
pub fn autostart(enabled: bool) -> Result<(), String> {
    unsafe {
        let mut h = std::ptr::null_mut();
        let code = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN).as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut h,
            std::ptr::null_mut(),
        );
        if code != 0 {
            return Err(format!("启动项写入失败: {code}"));
        }
        let code = if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let value = wide(&format!("\"{}\" --tray", exe.display()));
            RegSetValueExW(
                h,
                wide(NAME).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(h, wide(NAME).as_ptr())
        };
        RegCloseKey(h);
        if code != 0 && code != ERROR_FILE_NOT_FOUND {
            return Err(format!("启动项写入失败: {code}"));
        }
        Ok(())
    }
}
pub fn autostart_enabled() -> bool {
    unsafe {
        let mut b = [0u16; 32768];
        let mut size = (b.len() * 2) as u32;
        let mut kind = 0;
        let code = RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN).as_ptr(),
            wide(NAME).as_ptr(),
            RRF_RT_REG_SZ,
            &mut kind,
            b.as_mut_ptr().cast(),
            &mut size,
        );
        let expected = format!("\"{}\" --tray", std::env::current_exe().unwrap().display());
        code == 0
            && String::from_utf16_lossy(&b[..b.iter().position(|c| *c == 0).unwrap_or(b.len())])
                == expected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prior_settings_keep_preferences_with_optional_tray_mode() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"automatic":true,"verified_versions":["1.2.3.0"]}"#).unwrap();
        assert!(!settings.tray_only);
        assert!(!settings.look_at_mouse);
        settings.tray_only = true;
        let saved = serde_json::to_string(&settings).unwrap();
        let loaded: Settings = serde_json::from_str(&saved).unwrap();
        assert!(loaded.tray_only && loaded.automatic);
        assert_eq!(loaded.verified_versions, ["1.2.3.0"]);
    }
    #[test]
    fn confirming_version_preserves_automatic_preference_across_reload() {
        for automatic in [false, true] {
            let mut settings = Settings {
                automatic,
                tray_only: false,
                look_at_mouse: true,
                verified_versions: vec![],
            };
            settings.confirm_version("1.2.3.0");
            settings.confirm_version("1.2.3.0");
            let reloaded: Settings =
                serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(reloaded.automatic, automatic);
            assert!(reloaded.look_at_mouse);
            assert_eq!(reloaded.verified_versions, ["1.2.3.0"]);
        }
    }
}
