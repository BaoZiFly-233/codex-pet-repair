use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::Packaging::Appx::GetPackageFamilyName,
    System::{
        Diagnostics::ToolHelp::*, RemoteDesktop::ProcessIdToSessionId, StationsAndDesktops::*,
        Threading::*,
    },
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn text(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())])
}
pub fn error(operation: &str) -> String {
    format!("{operation}: Win32 {}", unsafe { GetLastError() })
}
pub struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                CloseHandle(self.0);
            }
        }
    }
}
impl Handle {
    pub fn process(pid: u32) -> Result<Self, String> {
        let p = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x100000, 0, pid) };
        if p.is_null() {
            Err(error("OpenProcess"))
        } else {
            Ok(Self(p))
        }
    }
    pub fn alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
    }
}
pub fn process_start(p: HANDLE) -> Result<u64, String> {
    unsafe {
        let (mut a, mut b, mut c, mut d) = (zeroed(), zeroed(), zeroed(), zeroed());
        if GetProcessTimes(p, &mut a, &mut b, &mut c, &mut d) == 0 {
            return Err(error("GetProcessTimes"));
        }
        Ok((a.dwHighDateTime as u64) << 32 | a.dwLowDateTime as u64)
    }
}
pub fn process_path(p: HANDLE) -> Result<String, String> {
    let mut buf = vec![0; 32768];
    let mut len = buf.len() as u32;
    if unsafe { QueryFullProcessImageNameW(p, 0, buf.as_mut_ptr(), &mut len) } == 0 {
        return Err(error("QueryFullProcessImageName"));
    }
    Ok(String::from_utf16_lossy(&buf[..len as usize]))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Owner {
    pub pid: u32,
    pub start: u64,
    pub version: String,
    pub path: String,
    pub test: bool,
}
fn owner(pid: u32, test: bool) -> Result<Owner, String> {
    unsafe {
        let mut session = 0;
        let mut own_session = 0;
        if ProcessIdToSessionId(pid, &mut session) == 0
            || ProcessIdToSessionId(GetCurrentProcessId(), &mut own_session) == 0
            || session != own_session
        {
            return Err("wrong_session".into());
        }
    }
    let p = Handle::process(pid)?;
    let path = process_path(p.0)?;
    if test {
        if !path.eq_ignore_ascii_case(
            &std::env::current_exe()
                .map_err(|e| e.to_string())?
                .to_string_lossy(),
        ) {
            return Err("not_test_process".into());
        }
        return Ok(Owner {
            pid,
            start: process_start(p.0)?,
            version: "synthetic".into(),
            path,
            test,
        });
    }
    let mut buf = vec![0; 256];
    let mut len = buf.len() as u32;
    if unsafe { GetPackageFamilyName(p.0, &mut len, buf.as_mut_ptr()) } != 0
        || text(&buf) != "OpenAI.Codex_2p2nqsd0c76g0"
    {
        return Err("not_official_package".into());
    }
    let lower = path.to_ascii_lowercase();
    if !lower.ends_with("\\app\\chatgpt.exe") || !lower.contains("\\windowsapps\\openai.codex_") {
        return Err("unsupported_path".into());
    }
    let version = lower
        .split("openai.codex_")
        .nth(1)
        .unwrap_or("")
        .split('_')
        .next()
        .unwrap_or("")
        .to_string();
    Ok(Owner {
        pid,
        start: process_start(p.0)?,
        version,
        path,
        test: false,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Window {
    pub hwnd: usize,
    pub owner: Owner,
    pub rect: [i32; 4],
    pub style: u32,
}
impl Window {
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.owner.pid, self.owner.start, self.hwnd)
    }
}
pub fn style(hwnd: usize) -> Result<u32, String> {
    unsafe {
        SetLastError(0);
        let v = GetWindowLongPtrW(hwnd as HWND, GWL_EXSTYLE);
        if v == 0 && GetLastError() != 0 {
            Err(error("GetWindowLongPtr"))
        } else {
            Ok(v as u32)
        }
    }
}
pub fn set_style(hwnd: usize, value: u32) -> Result<(), String> {
    unsafe {
        SetLastError(0);
        let v = SetWindowLongPtrW(hwnd as HWND, GWL_EXSTYLE, value as isize);
        if v == 0 && GetLastError() != 0 {
            Err(error("SetWindowLongPtr"))
        } else {
            Ok(())
        }
    }
}
pub fn refresh(hwnd: usize) -> Result<(), String> {
    if unsafe {
        SetWindowPos(
            hwnd as HWND,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    } == 0
    {
        Err(error("SetWindowPos"))
    } else {
        Ok(())
    }
}
pub fn same(w: &Window) -> bool {
    let mut pid = 0;
    unsafe {
        if IsWindow(w.hwnd as HWND) == 0 {
            return false;
        }
        GetWindowThreadProcessId(w.hwnd as HWND, &mut pid);
    }
    pid == w.owner.pid
        && owner(pid, w.owner.test)
            .is_ok_and(|p| p.start == w.owner.start && p.path.eq_ignore_ascii_case(&w.owner.path))
}
pub fn pointer_pressed_in(w: &Window) -> bool {
    unsafe {
        let pressed = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
            .iter()
            .any(|k| (GetAsyncKeyState(*k as i32) as u16 & 0x8000) != 0);
        if !pressed {
            return false;
        }
        let mut point: POINT = zeroed();
        let mut rect: RECT = zeroed();
        GetCursorPos(&mut point) != 0
            && GetWindowRect(w.hwnd as HWND, &mut rect) != 0
            && point.x >= rect.left
            && point.x < rect.right
            && point.y >= rect.top
            && point.y < rect.bottom
    }
}
pub fn desktop() -> bool {
    unsafe {
        fn name(h: HDESK) -> String {
            let mut b = [0u16; 256];
            let mut n = 0;
            if unsafe { GetUserObjectInformationW(h, UOI_NAME, b.as_mut_ptr().cast(), 512, &mut n) }
                == 0
            {
                String::new()
            } else {
                text(&b)
            }
        }
        let input = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if input.is_null() {
            return false;
        }
        let n = name(input);
        let current = name(GetThreadDesktop(GetCurrentThreadId()));
        CloseDesktop(input);
        !n.is_empty() && n == current
    }
}
#[derive(Default)]
pub struct Discovery {
    pub owners: HashMap<u32, Owner>,
}
impl Discovery {
    pub fn refresh_processes(&mut self) {
        let mut found = HashMap::new();
        unsafe {
            let h = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
            if h.0 == INVALID_HANDLE_VALUE {
                return;
            }
            let mut p: PROCESSENTRY32W = zeroed();
            p.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(h.0, &mut p);
            while ok != 0 {
                if text(&p.szExeFile).eq_ignore_ascii_case("ChatGPT.exe") {
                    let cached = self.owners.get(&p.th32ProcessID).filter(|o| {
                        Handle::process(o.pid)
                            .is_ok_and(|p| p.alive() && process_start(p.0).ok() == Some(o.start))
                    });
                    if let Some(o) = cached
                        .cloned()
                        .or_else(|| owner(p.th32ProcessID, false).ok())
                    {
                        found.insert(o.pid, o);
                    }
                }
                ok = Process32NextW(h.0, &mut p);
            }
        }
        self.owners = found;
    }
    pub fn scan(&self) -> Vec<Window> {
        scan_owners(&self.owners)
    }
}
pub fn scan_test(pid: u32) -> Vec<Window> {
    let owners = owner(pid, true)
        .map(|o| HashMap::from([(pid, o)]))
        .unwrap_or_default();
    scan_owners(&owners)
}
fn scan_owners(owners: &HashMap<u32, Owner>) -> Vec<Window> {
    struct Data<'a> {
        owners: &'a HashMap<u32, Owner>,
        windows: Vec<Window>,
    }
    unsafe extern "system" fn callback(hwnd: HWND, l: isize) -> i32 {
        let data = &mut *(l as *mut Data);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let Some(o) = data.owners.get(&pid) else {
            return 1;
        };
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut class = [0; 256];
        GetClassNameW(hwnd, class.as_mut_ptr(), 256);
        let expected = if o.test {
            "PetRepair.TestOverlay"
        } else {
            "Chrome_WidgetWin_1"
        };
        if text(&class) != expected {
            return 1;
        }
        let Ok(s) = style(hwnd as usize) else {
            return 1;
        };
        let mask = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED;
        if s & mask != mask {
            return 1;
        }
        let mut r: RECT = zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 || r.right <= r.left || r.bottom <= r.top {
            return 1;
        }
        data.windows.push(Window {
            hwnd: hwnd as usize,
            owner: o.clone(),
            style: s,
            rect: [r.left, r.top, r.right, r.bottom],
        });
        1
    }
    let mut data = Data {
        owners,
        windows: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(callback), &mut data as *mut _ as isize);
    }
    data.windows
}
pub struct Gate(Handle);
impl Gate {
    pub fn acquire(name: &str) -> Result<Self, String> {
        unsafe {
            let h = Handle(CreateMutexW(null(), 0, wide(name).as_ptr()));
            if h.0.is_null() {
                return Err(error("CreateMutex"));
            }
            match WaitForSingleObject(h.0, 0) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(h)),
                _ => Err("other_repair_running".into()),
            }
        }
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0 .0);
        }
    }
}
pub fn community_running() -> bool {
    unsafe {
        let h = Handle(OpenMutexW(
            0x100000,
            0,
            wide("Local\\ChatGPTOverlayFixWatcherV3").as_ptr(),
        ));
        !h.0.is_null()
    }
}
pub const REPAIR_LOCK: &str = "Local\\CodexTweaksPetDragRecoveryV1";
pub fn marker_set(hwnd: usize, name: &str) -> Result<(), String> {
    if unsafe { SetPropW(hwnd as HWND, wide(name).as_ptr(), 1usize as HANDLE) } == 0 {
        Err(error("SetProp"))
    } else {
        Ok(())
    }
}
pub fn marker_matches(hwnd: usize, name: &str) -> bool {
    unsafe { GetPropW(hwnd as HWND, wide(name).as_ptr()) == 1usize as HANDLE }
}
pub fn marker_remove(hwnd: usize, name: &str) {
    unsafe {
        RemovePropW(hwnd as HWND, wide(name).as_ptr());
    }
}

pub struct ProcessWatch {
    stop: Handle,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl ProcessWatch {
    pub fn start(pid: u32, window: usize, message: u32) -> Option<Self> {
        let stop = Handle(unsafe { CreateEventW(null(), 1, 0, null()) });
        if stop.0.is_null() {
            return None;
        }
        let signal = stop.0 as usize;
        let thread = std::thread::spawn(move || {
            let Ok(process) = Handle::process(pid) else {
                return;
            };
            let handles = [process.0, signal as HANDLE];
            if unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) } == WAIT_OBJECT_0
            {
                unsafe {
                    PostMessageW(window as HWND, message, 0, 0);
                }
            }
        });
        Some(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for ProcessWatch {
    fn drop(&mut self) {
        unsafe {
            SetEvent(self.stop.0);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
