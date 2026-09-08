use crate::{
    gaze_transport::{Local, Socket, PORT},
    native, storage,
};
use serde_json::{json, Value};
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    NetworkManagement::IpHelper::*,
    Networking::WinSock::AF_INET,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

pub const MESSAGE: u32 = WM_APP + 4;
pub const INPUT_MESSAGE: u32 = WM_APP + 5;
pub struct InputSample {
    pub window: native::Window,
    pub health: crate::policy::InputHealth,
    pub observed: Instant,
}
#[cfg(test)]
mod coordinator_tests {
    use super::*;
    #[test]
    fn a_previous_pause_ack_cannot_authorize_a_new_repair() {
        let (tx, _rx) = mpsc::channel();
        let manager = Manager {
            tx,
            state: Arc::new(Mutex::new(Status::default())),
            input: Arc::new(Mutex::new(None)),
            suspended: Arc::new(AtomicU64::new(u64::MAX)),
            requested: AtomicU64::new(0),
        };
        assert!(!manager.suspended());
        manager.configure(true, true, true);
        manager.suspended.store(1, Ordering::Release);
        assert!(manager.suspended());
        manager.configure(true, true, false);
        manager.configure(true, true, true);
        manager.suspended.store(1, Ordering::Release); // delayed response from the first pause
        assert!(!manager.suspended());
        manager.suspended.store(3, Ordering::Release);
        assert!(manager.suspended());
    }
}
const SCRIPT: &str = include_str!("../assets/gaze-bridge.js");
const STOP: &str = "window.__petRepairGazeV1?.stop(); true";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub code: String,
    pub text: String,
    pub detail: String,
}
impl Default for Status {
    fn default() -> Self {
        status("disabled", "")
    }
}
fn status(code: &str, detail: &str) -> Status {
    let text = match code {
        "disabled" => "看向鼠标已关闭",
        "checking" => "正在检查宠物与本机连接…",
        "ready" => "已连接 · 移动宠物周围的鼠标",
        "paused" => "暂时暂停跟随 · 修复完成或桌面恢复后继续",
        "debug_required" => "需要跟随模式 · 完全退出 Codex 后，点击下方启动",
        "codex_running" => "Codex 仍在运行 · 从其菜单完全退出后再启动",
        "launching" => "正在启动 Codex · 打开 V2 宠物后自动连接",
        "no_pet" => "未找到宠物 · 请在 Codex 中打开 V2 宠物",
        "ambiguous" => "找到多个宠物窗口 · 请只保留一个",
        "sprite_v2_required" => "需要 V2 宠物 · 请选择含方向帧的 V2 素材",
        "viewport_mismatch" => "窗口坐标不一致 · 暂停跟随，请重新打开宠物",
        "identity_rejected" => "连接身份不符 · 已停止跟随，可打开日志查看原因",
        "disconnected" => "连接已断开 · 将在宠物就绪后重连",
        "launch_failed" => "无法启动 Codex · 请打开日志查看原因",
        _ => "此客户端接口暂不兼容 · 可复制诊断摘要",
    };
    Status {
        code: code.into(),
        text: text.into(),
        detail: detail.into(),
    }
}
fn display_status(report: Status, gaze: bool) -> Status {
    if gaze {
        report
    } else {
        status(
            "disabled",
            &format!("输入检查：{}\n{}", report.code, report.detail),
        )
    }
}
enum Command {
    Configure(bool, bool, bool, u64),
    Launch,
    Quit,
}
pub struct Manager {
    tx: mpsc::Sender<Command>,
    state: Arc<Mutex<Status>>,
    input: Arc<Mutex<Option<InputSample>>>,
    suspended: Arc<AtomicU64>,
    requested: AtomicU64,
}
impl Manager {
    pub fn new(hwnd: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Status::default()));
        let output = state.clone();
        let input = Arc::new(Mutex::new(None));
        let samples = input.clone();
        let suspended = Arc::new(AtomicU64::new(u64::MAX));
        let ack = suspended.clone();
        std::thread::spawn(move || worker(rx, output, samples, ack, hwnd));
        Self {
            tx,
            state,
            input,
            suspended,
            requested: AtomicU64::new(0),
        }
    }
    pub fn configure(&self, enabled: bool, monitor: bool, paused: bool) {
        let generation = self.requested.fetch_add(1, Ordering::AcqRel) + 1;
        if enabled {
            let mut state = self.state.lock().unwrap();
            if state.code == "disabled" {
                *state = status("checking", "");
            }
        }
        let _ = self
            .tx
            .send(Command::Configure(enabled, monitor, paused, generation));
    }
    pub fn suspended(&self) -> bool {
        self.suspended.load(Ordering::Acquire) == self.requested.load(Ordering::Acquire)
    }
    pub fn take_input(&self) -> Option<InputSample> {
        self.input.lock().unwrap().take()
    }
    pub fn launch(&self) {
        let _ = self.tx.send(Command::Launch);
    }
    pub fn status(&self) -> Status {
        self.state.lock().unwrap().clone()
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
    }
}
fn publish(state: &Mutex<Status>, hwnd: usize, next: Status) {
    let mut current = state.lock().unwrap();
    if current.code == next.code && current.text == next.text {
        *current = next;
        return;
    }
    storage::event(
        "GAZE_STATE",
        None,
        &next.text,
        &format!("原因：{}\n{}", next.code, next.detail),
    );
    *current = next;
    unsafe {
        PostMessageW(hwnd as HWND, MESSAGE, 0, 0);
    }
}

// Verify the listener itself, not just the image name or a server-supplied CDP URL.
fn listener_pid() -> Result<u32, String> {
    unsafe {
        let mut length = 0;
        let first = GetExtendedTcpTable(
            null_mut(),
            &mut length,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if first != ERROR_INSUFFICIENT_BUFFER || length > 4 * 1024 * 1024 {
            return Err("tcp_table_unavailable".into());
        }
        let mut table = vec![0u32; (length as usize).div_ceil(4)];
        let capacity = table.len() * 4;
        if GetExtendedTcpTable(
            table.as_mut_ptr() as _,
            &mut length,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        ) != 0
        {
            return Err("tcp_table_changed".into());
        }
        let count = table[0] as usize;
        if count > capacity.saturating_sub(4) / size_of::<MIB_TCPROW_OWNER_PID>() {
            return Err("tcp_table_invalid".into());
        }
        let rows =
            std::slice::from_raw_parts(table.as_ptr().add(1) as *const MIB_TCPROW_OWNER_PID, count);
        let matching: Vec<_> = rows
            .iter()
            .filter(|r| u16::from_be(r.dwLocalPort as u16) == PORT)
            .collect();
        if matching.is_empty() {
            return Err("debug_required".into());
        }
        if matching.len() != 1 || matching[0].dwLocalAddr.to_ne_bytes() != [127, 0, 0, 1] {
            return Err("listener_not_exclusive_loopback".into());
        }
        Ok(matching[0].dwOwningPid)
    }
}
struct Connection {
    socket: Socket,
    _http: Local,
    window: native::Window,
    process: native::Handle,
    checked: Instant,
    resizes: u64,
}
impl Connection {
    fn connect() -> Result<Self, Status> {
        let pid = listener_pid().map_err(|e| {
            status(
                if e == "debug_required" {
                    "debug_required"
                } else {
                    "identity_rejected"
                },
                &e,
            )
        })?;
        let owner = native::owner(pid, false).map_err(|e| status("identity_rejected", &e))?;
        let process = native::Handle::process(pid).map_err(|e| status("identity_rejected", &e))?;
        if native::process_start(process.0).ok() != Some(owner.start) {
            return Err(status("identity_rejected", "process_reused"));
        }
        let mut discovery = native::Discovery::default();
        discovery.refresh_processes();
        let windows = discovery.scan();
        if windows.is_empty() {
            return Err(status(
                "no_pet",
                &format!("Codex {} · PID {pid}", owner.version),
            ));
        }
        if windows.len() != 1 {
            return Err(status("ambiguous", &format!("候选窗口：{}", windows.len())));
        }
        let window = windows[0].clone();
        if window.owner.pid != pid || window.owner.start != owner.start {
            return Err(status(
                "identity_rejected",
                "listener_window_owner_mismatch",
            ));
        }
        let http = Local::new().map_err(|e| status("disconnected", &e))?;
        let targets = http.targets().map_err(|e| status("disconnected", &e))?;
        let targets: Vec<_> = targets
            .iter()
            .filter(|t| {
                t["type"] == "page"
                    && t["url"]
                        .as_str()
                        .is_some_and(crate::gaze_transport::overlay_url)
            })
            .collect();
        if targets.is_empty() {
            return Err(status("no_pet", "宠物页面：0"));
        }
        if targets.len() != 1 {
            return Err(status("ambiguous", &format!("宠物页面：{}", targets.len())));
        }
        let url = targets[0]["webSocketDebuggerUrl"]
            .as_str()
            .ok_or_else(|| status("disconnected", "websocket_url_missing"))?;
        if listener_pid().ok() != Some(pid) || !process.alive() {
            return Err(status("identity_rejected", "listener_changed"));
        }
        let mut socket = http.attach(url).map_err(|e| status("disconnected", &e))?;
        if listener_pid().ok() != Some(pid) || !process.alive() || !native::same(&window) {
            return Err(status("identity_rejected", "target_changed_during_connect"));
        }
        let probe = socket
            .evaluate(SCRIPT)
            .map_err(|e| status("disconnected", &e))?;
        if probe["code"] != "ready" {
            return Err(status(
                probe["code"].as_str().unwrap_or("probe_failed"),
                &format!(
                    "Codex {} · PID {pid} · HWND {} · 样式 0x{:08X}",
                    owner.version, window.hwnd, window.style
                ),
            ));
        }
        Ok(Self {
            socket,
            _http: http,
            window,
            process,
            checked: Instant::now(),
            resizes: 0,
        })
    }
    fn pulse(&mut self, paused: bool, gaze: bool) -> Result<(Status, InputSample), Status> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(self.window.hwnd as HWND, &mut pid);
        }
        if !self.process.alive() || pid != self.window.owner.pid {
            return Err(status("identity_rejected", "target_identity_changed"));
        }
        if self.checked.elapsed() >= Duration::from_secs(3) {
            if listener_pid().ok() != Some(self.window.owner.pid) {
                return Err(status("identity_rejected", "listener_changed"));
            }
            let mut discovery = native::Discovery::default();
            discovery.refresh_processes();
            let windows = discovery.scan();
            if !paused && (windows.len() != 1 || windows[0].hwnd != self.window.hwnd) {
                return Err(status("ambiguous", "window_set_changed"));
            }
            self.checked = Instant::now();
        }
        let mut input = if paused || !native::desktop() {
            json!({"paused":true})
        } else {
            if native::inspect_window(&self.window.owner, self.window.hwnd).is_none() {
                return Err(status("identity_rejected", "window_style_changed"));
            }
            viewport(self.window.hwnd)
                .ok_or_else(|| status("disconnected", "viewport_unavailable"))?
        };
        input["gaze"] = json!(gaze);
        let response = self
            .socket
            .evaluate(&format!(
                "window.__petRepairGazeV1?.check({input}) ?? ({{code:'stopped'}})"
            ))
            .map_err(|e| status("disconnected", &e))?;
        let code = response["code"].as_str().unwrap_or("invalid_response");
        let resizes = response["resizes"].as_u64().unwrap_or(self.resizes);
        if resizes != self.resizes {
            self.resizes = resizes;
            storage::event(
                "GAZE_VIEWPORT_RESET",
                None,
                "宠物窗口尺寸变化，已清理旧视线坐标",
                "下一次真实鼠标移动会按新窗口位置计算视线。尺寸变化本身不会触发重复修复。",
            );
        }
        let detail = format!("Codex {} · PID {} · HWND {} · 样式 0x{:08X}\n连接：127.0.0.1:{PORT} · 真实鼠标事件驱动 · 连接检查：1 秒\n本次连接收到鼠标移动：{} · 窗口尺寸变化：{}", self.window.owner.version, self.window.owner.pid, self.window.hwnd, self.window.style, response["moves"].as_u64().unwrap_or(0), self.resizes);
        let mut report = status(code, &detail);
        if matches!(
            code,
            "ready" | "paused" | "viewport_mismatch" | "sprite_v2_required"
        ) {
            let window = native::inspect_window(&self.window.owner, self.window.hwnd)
                .unwrap_or_else(|| self.window.clone());
            let health = if code == "ready" && response["inputRegion"] == true {
                input_health(self.window.hwnd, &input)
            } else {
                crate::policy::InputHealth::Unknown
            };
            report.detail.push_str(&format!(
                "\n输入检查：{}",
                match health {
                    crate::policy::InputHealth::Healthy => "实际命中宠物，无需重复修复",
                    crate::policy::InputHealth::Mismatch => "实体交互区域未命中宠物，继续检查",
                    _ => "条件不足，未判定故障（需实体区域、无按键、无遮挡且坐标稳定）",
                }
            ));
            Ok((
                report,
                InputSample {
                    window,
                    health,
                    observed: Instant::now(),
                },
            ))
        } else {
            Err(report)
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.socket.evaluate(STOP);
    }
}
struct PhysicalDpi(DPI_AWARENESS_CONTEXT);
impl PhysicalDpi {
    fn enter() -> Option<Self> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        (!previous.is_null()).then(|| Self(previous))
    }
}
impl Drop for PhysicalDpi {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}
fn viewport(hwnd: usize) -> Option<Value> {
    let _dpi = PhysicalDpi::enter()?;
    unsafe {
        let mut point: POINT = zeroed();
        let mut rect: RECT = zeroed();
        let mut cursor: POINT = zeroed();
        if windows_sys::Win32::Graphics::Gdi::ClientToScreen(hwnd as HWND, &mut point) == 0
            || GetClientRect(hwnd as HWND, &mut rect) == 0
            || GetCursorPos(&mut cursor) == 0
        {
            return None;
        }
        Some(
            json!({"paused":false,"x":point.x,"y":point.y,"width":rect.right-rect.left,"height":rect.bottom-rect.top,"cursor":{"x":cursor.x,"y":cursor.y}}),
        )
    }
}
fn input_health(hwnd: usize, before: &Value) -> crate::policy::InputHealth {
    use crate::policy::InputHealth::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let Some(_dpi) = PhysicalDpi::enter() else {
        return Unknown;
    };
    let Some(after) = viewport(hwnd) else {
        return Unknown;
    };
    if ["x", "y", "width", "height", "cursor"]
        .iter()
        .any(|k| before[*k] != after[*k])
    {
        return Unknown;
    }
    unsafe {
        if [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
            .iter()
            .any(|k| (GetAsyncKeyState(*k as i32) as u16 & 0x8000) != 0)
        {
            return Unknown;
        }
        let point = POINT {
            x: after["cursor"]["x"].as_i64().unwrap() as i32,
            y: after["cursor"]["y"].as_i64().unwrap() as i32,
        };
        let target = hwnd as HWND;
        if GetAncestor(WindowFromPoint(point), GA_ROOT) == target {
            return Healthy;
        }
        // A different window above the pet can legitimately own the pointer.
        // Ambiguous occlusion is unknown, never a reason to reset the pet.
        let mut above = GetWindow(target, GW_HWNDPREV);
        for _ in 0..512 {
            if above.is_null() {
                return Mismatch;
            }
            let mut r: RECT = zeroed();
            if IsWindowVisible(above) != 0
                && IsIconic(above) == 0
                && GetWindowRect(above, &mut r) != 0
                && point.x >= r.left
                && point.x < r.right
                && point.y >= r.top
                && point.y < r.bottom
            {
                return Unknown;
            }
            above = GetWindow(above, GW_HWNDPREV);
        }
        Unknown
    }
}
fn launch() -> Result<(), String> {
    let mut discovery = native::Discovery::default();
    discovery.refresh_processes();
    if !discovery.owners.is_empty() {
        return Err("codex_running".into());
    }
    let reservation = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, PORT))
        .map_err(|_| "调试端口被占用")?;
    drop(reservation);
    activate_codex(&format!(
        "--remote-debugging-port={PORT} --remote-debugging-address=127.0.0.1"
    ))?;
    Ok(())
}

// Activate the registered MSIX application; launching its protected executable directly
// can fail with access denied. Interface layout follows shobjidl_core.h.
fn activate_codex(arguments: &str) -> Result<u32, String> {
    use std::ffi::c_void;
    use windows_sys::{core::GUID, Win32::System::Com::*};
    #[repr(C)]
    struct ActivationVtable {
        query: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        activate:
            unsafe extern "system" fn(*mut c_void, *const u16, *const u16, u32, *mut u32) -> i32,
    }
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    unsafe {
        let hr = CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED as u32);
        if hr < 0 {
            return Err(format!("CoInitializeEx:0x{:08X}", hr as u32));
        }
        let _com = Com;
        let clsid = GUID::from_u128(0x45ba127d_10a8_46ea_8ab7_56ea9078943c);
        let iid = GUID::from_u128(0x2e941141_7f97_4756_ba1d_9decde894a3d);
        let mut manager = null_mut();
        let hr = CoCreateInstance(&clsid, null_mut(), CLSCTX_LOCAL_SERVER, &iid, &mut manager);
        if hr < 0 || manager.is_null() {
            return Err(format!("ActivationManager:0x{:08X}", hr as u32));
        }
        let vtable = &**(manager as *const *const ActivationVtable);
        let mut pid = 0;
        let hr = (vtable.activate)(
            manager,
            native::wide("OpenAI.Codex_2p2nqsd0c76g0!App").as_ptr(),
            native::wide(arguments).as_ptr(),
            2,
            &mut pid,
        );
        (vtable.release)(manager);
        if hr < 0 {
            Err(format!("ActivateApplication:0x{:08X}", hr as u32))
        } else {
            Ok(pid)
        }
    }
}
fn worker(
    rx: mpsc::Receiver<Command>,
    output: Arc<Mutex<Status>>,
    input: Arc<Mutex<Option<InputSample>>>,
    suspended: Arc<AtomicU64>,
    hwnd: usize,
) {
    let (mut enabled, mut monitor, mut paused) = (false, false, false);
    let mut generation = 0;
    let mut safe_after = Instant::now();
    let mut connection: Option<Connection> = None;
    let mut retry = Instant::now();
    loop {
        let command = if enabled || monitor || paused {
            rx.recv_timeout(Duration::from_secs(1))
        } else {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        match command {
            Ok(Command::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Command::Configure(on, observe, pause, epoch)) => {
                generation = epoch;
                enabled = on;
                monitor = observe;
                paused = pause;
                if !on && !observe {
                    if connection.is_some() {
                        safe_after = Instant::now() + Duration::from_secs(4);
                    }
                    connection = None;
                    publish(&output, hwnd, Status::default());
                }
                retry = Instant::now();
            }
            Ok(Command::Launch) => {
                if enabled {
                    match launch() {
                        Ok(()) => publish(
                            &output,
                            hwnd,
                            status("launching", "已请求以本机调试模式启动官方 Codex"),
                        ),
                        Err(e) => publish(
                            &output,
                            hwnd,
                            status(
                                if e == "codex_running" {
                                    "codex_running"
                                } else {
                                    "launch_failed"
                                },
                                &e,
                            ),
                        ),
                    }
                    retry = Instant::now() + Duration::from_secs(3);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let acknowledge_pause = |ready| {
            suspended.store(if ready { generation } else { 0 }, Ordering::Release);
        };
        if !enabled && !monitor {
            acknowledge_pause(paused && Instant::now() >= safe_after);
            continue;
        }
        if let Some(active) = &mut connection {
            match active.pulse(paused, enabled) {
                Ok((report, sample)) => {
                    acknowledge_pause(paused && report.code == "paused");
                    *input.lock().unwrap() = Some(sample);
                    unsafe {
                        PostMessageW(hwnd as HWND, INPUT_MESSAGE, 0, 0);
                    }
                    publish(&output, hwnd, display_status(report, enabled));
                }
                Err(report) => {
                    connection = None;
                    *input.lock().unwrap() = None;
                    safe_after = Instant::now() + Duration::from_secs(4);
                    acknowledge_pause(false);
                    publish(&output, hwnd, display_status(report, enabled));
                    retry = Instant::now() + Duration::from_secs(3);
                }
            }
        } else if Instant::now() >= retry && !paused && native::desktop() {
            match Connection::connect() {
                Ok(active) => {
                    connection = Some(active);
                }
                Err(report) => publish(&output, hwnd, display_status(report, enabled)),
            }
            retry = Instant::now() + Duration::from_secs(3);
        }
        if paused && connection.is_none() {
            acknowledge_pause(Instant::now() >= safe_after);
        }
    }
}
