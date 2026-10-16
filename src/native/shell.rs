use super::{MSG_TRAY, Result, wide, winerr};
use orbit::config::Config;
use std::{
    fs,
    mem::size_of,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Registry::*},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::*,
};

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
}
impl Tray {
    pub fn new(hwnd: HWND, icon: HICON) -> Result<Self> {
        let tray = Self { hwnd, icon };
        tray.update(NIM_ADD, "Orbit — ready")?;
        Ok(tray)
    }
    fn data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: MSG_TRAY,
            hIcon: self.icon,
            ..Default::default()
        }
    }
    pub fn update(&self, action: NOTIFY_ICON_MESSAGE, tip: &str) -> Result<()> {
        let mut data = self.data();
        copy(&mut data.szTip, tip);
        unsafe {
            if !Shell_NotifyIconW(action, &data).as_bool() {
                return Err("Windows could not create the Orbit tray icon.".into());
            }
        }
        Ok(())
    }
    pub fn notify(&self, text: &str) {
        let mut data = self.data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_INFO;
        copy(&mut data.szInfoTitle, "Orbit");
        copy(&mut data.szInfo, text);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
    pub fn menu(&self, paused: bool) -> usize {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return 0;
            };
            for (id, text) in [
                (1, "Settings…"),
                (
                    2,
                    if paused {
                        "Resume Orbit"
                    } else {
                        "Pause Orbit"
                    },
                ),
                (3, "Undo last arrangement\tWin+Alt+Z"),
                (4, "Restart as administrator…"),
                (5, "Quit Orbit"),
            ] {
                let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(wide(text).as_ptr()));
            }
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            let _ = SetForegroundWindow(self.hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                p.x,
                p.y,
                Some(0),
                self.hwnd,
                None,
            )
            .0 as usize;
            let _ = DestroyMenu(menu);
            let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            command
        }
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data());
        }
    }
}
fn copy<const N: usize>(dst: &mut [u16; N], text: &str) {
    for (a, b) in dst.iter_mut().take(N - 1).zip(text.encode_utf16()) {
        *a = b;
    }
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
pub fn startup_enabled() -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            Some(0),
            KEY_QUERY_VALUE,
            &mut key,
        )
        .is_err()
        {
            return false;
        }
        let mut len = 0;
        let status = RegQueryValueExW(key, w!("Orbit"), None, None, None, Some(&mut len));
        let _ = RegCloseKey(key);
        status.is_ok() && len > 2
    }
}
pub fn set_startup(enabled: bool) -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()
        .map_err(winerr)?;
        let result = if enabled {
            let installed = install_dir()?.join("Orbit.exe");
            let exe = if installed.is_file() {
                installed
            } else {
                std::env::current_exe().map_err(|e| e.to_string())?
            };
            let command = format!("\"{}\" --background", exe.display());
            let bytes: Vec<u8> = wide(&command)
                .iter()
                .flat_map(|n| n.to_le_bytes())
                .collect();
            RegSetValueExW(key, w!("Orbit"), None, REG_SZ, Some(&bytes))
        } else {
            RegDeleteValueW(key, w!("Orbit"))
        };
        let _ = RegCloseKey(key);
        if !enabled && result == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        result.ok().map_err(winerr)
    }
}

fn known(id: &GUID) -> Result<PathBuf> {
    unsafe {
        let value = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(winerr)?;
        let result = value.to_string().map_err(|e| e.to_string());
        CoTaskMemFree(Some(value.0 as _));
        result.map(PathBuf::from)
    }
}
pub fn install_dir() -> Result<PathBuf> {
    Ok(known(&FOLDERID_LocalAppData)?
        .join("Programs")
        .join("Orbit"))
}
pub fn desktop_shortcut() -> Result<()> {
    shortcut(
        &install_dir()?.join("Orbit.exe"),
        &known(&FOLDERID_Desktop)?.join("Orbit.lnk"),
        "",
    )
}
pub fn install() -> Result<PathBuf> {
    let folder = install_dir()?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = folder.join("Orbit.exe");
    if exe != target {
        let tmp = folder.join("Orbit.new.exe");
        fs::copy(&exe, &tmp).map_err(|e| e.to_string())?;
        fs::rename(tmp, &target)
            .map_err(|_| "Quit the installed copy of Orbit before updating it.".to_string())?;
    }
    let shortcuts = known(&FOLDERID_Programs)?.join("Orbit");
    fs::create_dir_all(&shortcuts).map_err(|e| e.to_string())?;
    shortcut(&target, &shortcuts.join("Orbit.lnk"), "")?;
    shortcut(
        &target,
        &shortcuts.join("Uninstall Orbit.lnk"),
        "--uninstall",
    )?;
    register_installation(&folder, &target)?;
    if startup_enabled() {
        set_startup(true)?;
    }
    Ok(folder)
}
const UNINSTALL_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\SeffyOrbit");
fn register_installation(folder: &Path, exe: &Path) -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            UNINSTALL_KEY,
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()
        .map_err(winerr)?;
        let result = (|| -> Result<()> {
            for (name, value) in [
                ("DisplayName", "Orbit".to_string()),
                ("DisplayVersion", env!("CARGO_PKG_VERSION").to_string()),
                ("Publisher", "Seffy Fisher".to_string()),
                ("InstallLocation", folder.to_string_lossy().into_owned()),
                ("DisplayIcon", format!("\"{}\",0", exe.display())),
                (
                    "UninstallString",
                    format!("\"{}\" --uninstall", exe.display()),
                ),
            ] {
                let bytes: Vec<u8> = wide(&value).iter().flat_map(|n| n.to_le_bytes()).collect();
                RegSetValueExW(key, PCWSTR(wide(name).as_ptr()), None, REG_SZ, Some(&bytes))
                    .ok()
                    .map_err(winerr)?;
            }
            for name in ["NoModify", "NoRepair"] {
                RegSetValueExW(
                    key,
                    PCWSTR(wide(name).as_ptr()),
                    None,
                    REG_DWORD,
                    Some(&1u32.to_le_bytes()),
                )
                .ok()
                .map_err(winerr)?;
            }
            Ok(())
        })();
        let _ = RegCloseKey(key);
        result
    }
}
fn shortcut(exe: &Path, path: &Path, args: &str) -> Result<()> {
    unsafe {
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(winerr)?;
        link.SetPath(PCWSTR(wide(&exe.to_string_lossy()).as_ptr()))
            .map_err(winerr)?;
        link.SetArguments(PCWSTR(wide(args).as_ptr()))
            .map_err(winerr)?;
        link.SetDescription(w!("Orbit window manager"))
            .map_err(winerr)?;
        link.SetWorkingDirectory(PCWSTR(
            wide(&exe.parent().unwrap().to_string_lossy()).as_ptr(),
        ))
        .map_err(winerr)?;
        let file: IPersistFile = link.cast().map_err(winerr)?;
        file.Save(PCWSTR(wide(&path.to_string_lossy()).as_ptr()), true)
            .map_err(winerr)
    }
}
pub fn uninstall() -> Result<()> {
    // Deletion occurs after this process exits. Paths are literal PowerShell strings.
    let folder = install_dir()?;
    let shortcut_folder = known(&FOLDERID_Programs)?.join("Orbit");
    let desktop_link = known(&FOLDERID_Desktop)?.join("Orbit.lnk");
    if folder.file_name().and_then(|s| s.to_str()) != Some("Orbit") {
        return Err("Unexpected installation path.".into());
    }
    set_startup(false)?;
    unsafe {
        let status = RegDeleteTreeW(HKEY_CURRENT_USER, UNINSTALL_KEY);
        if status != ERROR_FILE_NOT_FOUND {
            status.ok().map_err(winerr)?;
        }
    }
    let script = Config::folder().join("uninstall.ps1");
    fs::create_dir_all(Config::folder()).map_err(|e| e.to_string())?;
    let literal = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
    let content = format!(
        "Start-Sleep -Seconds 2\nRemove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue\nRemove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue\nRemove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue\n",
        literal(&folder),
        literal(&shortcut_folder),
        literal(&desktop_link)
    );
    fs::write(&script, content).map_err(|e| e.to_string())?;
    use std::os::windows::process::CommandExt;
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn open_path(path: &Path) -> Result<()> {
    unsafe {
        let result = ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide(&path.to_string_lossy()).as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
        if result.0 as isize <= 32 {
            Err("Windows could not open this file or folder.".into())
        } else {
            Ok(())
        }
    }
}
pub fn open_report(path: &Path) -> Result<()> {
    let windows = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    if std::process::Command::new(windows.join("System32").join("notepad.exe"))
        .arg(path)
        .spawn()
        .is_ok()
    {
        return Ok(());
    }
    // A fresh Windows account may have no default application for .json files.
    open_path(
        path.parent()
            .ok_or("The diagnostic folder is unavailable.")?,
    )
}
pub fn elevate() -> Result<()> {
    unsafe {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let result = ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(wide(&exe.to_string_lossy()).as_ptr()),
            w!("--wait-for-exit"),
            None,
            SW_SHOWNORMAL,
        );
        if result.0 as isize <= 32 {
            Err("Administrator restart was cancelled or could not start.".into())
        } else {
            Ok(())
        }
    }
}
pub fn log(text: &str) {
    let folder = Config::folder();
    if fs::create_dir_all(&folder).is_err() {
        return;
    }
    let path = folder.join("orbit.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > 128 * 1024) {
        let _ = fs::remove_file(&path);
    }
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "{} {text}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
    }
}
