use crate::{
    bridge,
    native::{self, wide, Window},
    policy::Policy,
    protocol::UiState,
    repair,
    signal::Signal,
    storage::{self, Settings},
};
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use windows_sys::{
    core::w,
    Win32::{
        Foundation::*,
        System::LibraryLoader::*,
        UI::{Accessibility::*, Shell::*, WindowsAndMessaging::*},
    },
};
static ROOT: AtomicUsize = AtomicUsize::new(0);
const TRAY: u32 = WM_APP + 1;
const EVENT: u32 = WM_APP + 2;
const PROCESS_EXIT: u32 = WM_APP + 3;
const FIX: u16 = 101;
const AUTO: u16 = 102;
const START: u16 = 103;
const DETAIL: u16 = 104;
const YES: u16 = 105;
const NO: u16 = 106;
const EXIT: u16 = 107;
const OPEN: u16 = 108;
const TOGGLE_AUTO: u16 = 111;
const TRAY_ONLY: u16 = 112;
const COPY_DIAGNOSTICS: u16 = 113;
const OPEN_LOG_FOLDER: u16 = 114;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
fn arm_event_batch(due: &mut Option<u64>, now: u64) -> bool {
    if due.is_some() {
        false
    } else {
        *due = Some(now + 300);
        true
    }
}
struct App {
    hwnd: HWND,
    windows: Vec<Window>,
    discovery: native::Discovery,
    policy: Policy,
    settings: Settings,
    clock: Instant,
    running: Option<repair::Running>,
    target: Option<Window>,
    last: Option<repair::Report>,
    message: String,
    hooks: Vec<HWINEVENTHOOK>,
    pids: Vec<(u32, u64)>,
    closing: bool,
    tray: bool,
    taskbar: u32,
    icon: HICON,
    process_watches: Vec<native::ProcessWatch>,
    awaiting_confirmation: bool,
    ui_process: Option<std::process::Child>,
    wait_until: Option<u64>,
    last_operation: String,
    last_error: String,
    action_error: Option<String>,
    autostart: bool,
    reconcile_pending: bool,
    dirty: std::collections::HashSet<usize>,
    events_due: Option<u64>,
    state_signal: Option<Signal>,
    published: Option<UiState>,
    published_wait: Option<u64>,
    revision: u64,
    ui_queries: u64,
}
impl App {
    fn new() -> Self {
        Self {
            hwnd: null_mut(),
            windows: Vec::new(),
            discovery: Default::default(),
            policy: Default::default(),
            settings: Settings::load(),
            clock: Instant::now(),
            running: None,
            target: None,
            last: None,
            message: String::new(),
            hooks: Vec::new(),
            pids: Vec::new(),
            closing: false,
            tray: false,
            taskbar: 0,
            icon: null_mut(),
            process_watches: Vec::new(),
            awaiting_confirmation: false,
            ui_process: None,
            wait_until: None,
            last_operation: String::new(),
            last_error: String::new(),
            action_error: None,
            autostart: storage::autostart_enabled(),
            reconcile_pending: false,
            dirty: Default::default(),
            events_due: None,
            state_signal: Signal::new(Some(&crate::protocol::change_event(bridge::session()))).ok(),
            published: None,
            published_wait: None,
            revision: 0,
            ui_queries: 0,
        }
    }
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }
    unsafe fn init(&mut self) {
        self.icon = create_icon();
        self.taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
        self.add_tray();
        if let Some(report) = repair::recover_pending() {
            self.last = Some(report);
        }
        self.refresh(true);
        if let Err(error) = bridge::listen(self.hwnd as usize) {
            self.say(&error);
        }
        SetTimer(self.hwnd, 1, 10000, None);
        SetTimer(self.hwnd, 9, 30000, None);
        self.schedule_automatic();
    }
    unsafe fn say(&mut self, message: &str) {
        let message = if !message.is_empty()
            && !message
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        {
            if self.last_error != message {
                self.last_error = message.into();
                storage::event(
                    "ACTION_FAILED",
                    None,
                    "操作未能完成",
                    &format!("建议：复制诊断摘要用于排查\n原始错误：{message}"),
                );
            }
            "操作未完成 · 请打开日志或复制诊断摘要"
        } else {
            message
        };
        self.wait_until = None;
        if self.message != message {
            self.message = message.into();
            let mut n = self.tray_data();
            copy_wide(&mut n.szTip, &format!("浮窗修复 · {message}"));
            n.uFlags = NIF_TIP;
            Shell_NotifyIconW(NIM_MODIFY, &n);
        }
    }
    unsafe fn tray_data(&self) -> NOTIFYICONDATAW {
        let mut n: NOTIFYICONDATAW = zeroed();
        n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        n.hWnd = self.hwnd;
        n.uID = 1;
        n.uCallbackMessage = TRAY;
        n.hIcon = self.icon;
        n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        copy_wide(&mut n.szTip, "Codex 浮窗修复");
        n
    }
    unsafe fn add_tray(&mut self) {
        let mut n = self.tray_data();
        self.tray = Shell_NotifyIconW(NIM_ADD, &n) != 0;
        n.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &n);
    }
    unsafe fn sync_hooks(&mut self) {
        let mut pids: Vec<_> = self
            .discovery
            .owners
            .values()
            .map(|o| (o.pid, o.start))
            .collect();
        pids.sort();
        if pids == self.pids {
            return;
        }
        for h in self.hooks.drain(..) {
            UnhookWinEvent(h);
        }
        self.process_watches.clear();
        for (pid, _) in &pids {
            let hook = SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_HIDE,
                null_mut(),
                Some(event_callback),
                *pid,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            if !hook.is_null() {
                self.hooks.push(hook);
            }
            let location = SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                null_mut(),
                Some(event_callback),
                *pid,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            if !location.is_null() {
                self.hooks.push(location);
            }
            if let Some(w) = native::ProcessWatch::start(*pid, self.hwnd as usize, PROCESS_EXIT) {
                self.process_watches.push(w);
            }
        }
        self.pids = pids;
    }
    unsafe fn refresh(&mut self, processes: bool) {
        self.reconcile_pending |= processes;
        if self.running.is_some() {
            return;
        }
        if self.reconcile_pending {
            self.discovery.refresh_processes();
            self.sync_hooks();
            self.windows = self.discovery.scan();
            self.reconcile_pending = false;
            self.dirty.clear();
        } else {
            // Window events only inspect their HWNDs. Timer decisions recheck cached candidates.
            if self.dirty.is_empty() {
                for w in &self.windows {
                    self.dirty.insert(w.hwnd);
                }
            }
            for hwnd in self.dirty.drain() {
                self.discovery.update(hwnd);
            }
            self.windows = self.discovery.current();
        }
        self.policy.observe(&self.windows, self.now());
        if self.awaiting_confirmation {
            return;
        }
        if self.windows.is_empty() {
            self.say(if self.discovery.owners.is_empty() {
                "请先打开 Codex"
            } else {
                "请先显示宠物"
            });
        } else if self.windows.len() > 1 {
            self.say("检测到多个浮窗，请先隐藏其他浮窗");
        } else if self.last.is_none() {
            self.say("可以修复 · 点击“立即修复”");
        }
    }
    unsafe fn schedule_automatic(&mut self) {
        if self.settings.automatic && self.windows.len() == 1 {
            self.try_fix(false);
        }
    }
    unsafe fn try_fix(&mut self, manual: bool) {
        if self.running.is_some() || (!manual && self.awaiting_confirmation) {
            return;
        }
        if self.windows.len() != 1 {
            self.say("请仅保留需要修复的浮窗");
            return;
        }
        let w = self.windows[0].clone();
        if !manual && !self.settings.verified_versions.contains(&w.owner.version) {
            self.say("此版本需先手动修复并确认有效");
            return;
        }
        let now = self.now();
        if let Err(reason) = self.policy.allow(&w, now, manual) {
            self.say(reason.message());
            if !manual && reason.wait_ms().is_some() {
                self.say(&format!("自动修复等待中 · {}", reason.message()));
            }
            if let Some(ms) = reason.wait_ms() {
                self.wait_until = Some(now + ms);
                SetTimer(self.hwnd, 2, (ms + 50).min(300_100) as u32, None);
            }
            return;
        }
        if !native::desktop() {
            self.say("等待桌面可操作 · 解锁电脑或关闭系统安全提示后再试");
            SetTimer(self.hwnd, 2, 1000, None);
            return;
        }
        if native::pointer_pressed_in(&w) {
            self.say("等待宠物操作结束 · 松开宠物上的鼠标按键后继续");
            SetTimer(self.hwnd, 2, 1000, None);
            return;
        }
        let gate = native::Gate::acquire(native::REPAIR_LOCK);
        if gate.is_err() || native::community_running() {
            self.say("其他修复器正在运行，请先停用");
            return;
        }
        drop(gate);
        match repair::launch(w.clone()) {
            Ok(r) => {
                self.last_operation = r.operation_id.clone();
                storage::event(
                    "REPAIR_TRIGGER",
                    Some(&self.last_operation),
                    if manual {
                        "用户发起手动修复"
                    } else {
                        "自动修复已触发"
                    },
                    &format!(
                        "自动修复设置：{}",
                        if self.settings.automatic {
                            "开启"
                        } else {
                            "关闭"
                        }
                    ),
                );
                self.policy.attempted(now);
                self.running = Some(r);
                self.target = Some(w);
                self.last = None;
                self.awaiting_confirmation = false;
                self.say("正在修复 · 约需 3 秒，请暂时不要拖动宠物");
                SetTimer(self.hwnd, 3, 100, None);
            }
            Err(e) => {
                self.policy.halted = true;
                self.say(&e);
            }
        }
    }
    unsafe fn poll_result(&mut self) {
        let Some(r) = &self.running else {
            return;
        };
        let result = match r.result.try_recv() {
            Ok(r) => Some(r),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(repair::Report {
                code: "辅助进程通信中断".into(),
                restored: false,
                exact: false,
                elapsed_ms: 0,
            }),
        };
        if let Some(result) = result {
            self.running = None;
            KillTimer(self.hwnd, 3);
            if result.can_confirm() {
                let verified = self
                    .target
                    .as_ref()
                    .is_some_and(|w| self.settings.verified_versions.contains(&w.owner.version));
                if verified {
                    if let Some(w) = &self.target {
                        self.policy.complete(w);
                    }
                    self.say("窗口状态已重置 · 请检查宠物能否拖动");
                } else {
                    self.awaiting_confirmation = true;
                    self.policy.halted = true;
                    self.say("请检查修复效果 · 试着拖动宠物，再选择下面的结果");
                }
            } else if result.code == "cancelled_restored" {
                if let Some(w) = &self.target {
                    self.policy.complete(w);
                }
                self.say("修复已取消 · 窗口原状态已恢复");
            } else if result.code == "input_interrupted_restored" {
                if let Some(w) = &self.target {
                    self.policy.complete(w);
                }
                self.say("桌面切换已中断修复 · 原状态已恢复；回到正常桌面后可重试");
            } else {
                self.policy.halted = true;
                self.say(&format!(
                    "{} · {}",
                    result.description().1,
                    result.description().2
                ));
            }
            self.last = Some(result);
            self.refresh(false);
            if self.closing {
                DestroyWindow(self.hwnd);
            }
        } else if r.started.elapsed().as_secs() > 12 {
            r.cancel();
            self.say("修复超时，正在恢复原状");
        }
    }
    unsafe fn open_log(&mut self) -> Result<(), String> {
        storage::event("DIAGNOSTICS_OPENED", None, "打开日志", &self.detail_text());
        let log = storage::data_dir().join("events.log");
        let result = ShellExecuteW(
            self.hwnd,
            w!("open"),
            wide(&log.to_string_lossy()).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
        if result as isize <= 32 {
            let mut system = [0u16; 260];
            let n = windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
                system.as_mut_ptr(),
                system.len() as u32,
            );
            if n > 0 && (n as usize) < system.len() {
                let notepad =
                    std::path::PathBuf::from(String::from_utf16_lossy(&system[..n as usize]))
                        .join("notepad.exe");
                if std::process::Command::new(notepad)
                    .arg(&log)
                    .spawn()
                    .is_ok()
                {
                    return Ok(());
                }
            }
            return Err(format!(
                "无法启动日志查看程序。可复制诊断摘要，或从此位置读取日志：{}",
                log.display()
            ));
        }
        Ok(())
    }
    fn status_text(&self) -> String {
        if let Some(deadline) = self.wait_until {
            let remaining = deadline.saturating_sub(self.now()).div_ceil(1000);
            if remaining > 0 {
                return format!("{} · 还需 {remaining} 秒", self.message);
            }
            return "等待已结束 · 可以点击“立即修复”".into();
        }
        self.message.clone()
    }
    unsafe fn open_log_folder(&self) -> Result<(), String> {
        let folder = storage::data_dir();
        std::fs::create_dir_all(&folder).map_err(|e| format!("无法创建日志目录：{e}"))?;
        if ShellExecuteW(
            self.hwnd,
            w!("open"),
            wide(&folder.to_string_lossy()).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        ) as isize
            <= 32
        {
            return Err(format!("无法打开目录：{}", folder.display()));
        }
        Ok(())
    }
    unsafe fn copy_diagnostics(&self) -> Result<(), String> {
        use windows_sys::Win32::System::{DataExchange::*, Memory::*};
        let data = wide(&self.detail_text());
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2);
        if memory.is_null() {
            return Err("复制失败：无法分配内存".into());
        }
        let buffer = GlobalLock(memory) as *mut u16;
        if buffer.is_null() {
            GlobalFree(memory);
            return Err("复制失败：无法访问内存".into());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), buffer, data.len());
        GlobalUnlock(memory);
        if OpenClipboard(self.hwnd) == 0 {
            GlobalFree(memory);
            return Err("剪贴板正被其他程序使用，请稍后再试".into());
        }
        let succeeded = EmptyClipboard() != 0 && !SetClipboardData(13, memory).is_null();
        CloseClipboard();
        if !succeeded {
            GlobalFree(memory);
            return Err("无法写入剪贴板，请稍后再试".into());
        }
        Ok(())
    }
    fn automatic_status(&self) -> &'static str {
        if !self.settings.automatic {
            "自动修复已关闭"
        } else if self.awaiting_confirmation {
            "自动修复已开启 · 等待你确认效果"
        } else if self.policy.halted {
            "自动修复已开启 · 当前暂停处理"
        } else if self
            .windows
            .first()
            .is_some_and(|w| !self.settings.verified_versions.contains(&w.owner.version))
        {
            "自动修复已开启 · 此版本尚未确认有效"
        } else {
            "自动修复已开启"
        }
    }
    fn detail_text(&self) -> String {
        let result = self
            .last
            .as_ref()
            .map(|r| {
                format!(
                    "结果：{}\n建议：{}\n窗口原状态已恢复：{}\n耗时：{:.2} 秒\n事件编号：{}\n原始结果：{}",
                    r.description().1,r.description().2,if r.restored {"是"} else {"未确认"},r.elapsed_ms as f64/1000.0,r.description().0,r.code
                )
            })
            .unwrap_or("尚未执行恢复".into());
        let targets = self
            .windows
            .iter()
            .map(|w| {
                format!(
                    "版本 {} · PID {} · HWND {} · 样式 0x{:08X}",
                    w.owner.version, w.owner.pid, w.hwnd, w.style
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let info = format!(
            "Codex 宠物修复 {} · {}\n生成时间：{}\n当前状态：{}\n{}\n修复编号：{}\n\n{}\n\n技术诊断：\n{}",
            env!("CARGO_PKG_VERSION"),std::env::consts::ARCH,storage::timestamp(),self.status_text(),self.automatic_status(),if self.last_operation.is_empty() {"尚无"} else {&self.last_operation},
            result,
            targets
        );
        if self.last_error.is_empty() {
            info
        } else {
            format!("{info}\n\n最近程序错误：\n{}", self.last_error)
        }
    }
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "message":self.status_text(),"automatic_status":self.automatic_status(),"automatic":self.settings.automatic,
            "autostart":self.autostart,"tray_only":self.settings.tray_only,
            "busy":self.running.is_some(),"can_repair":self.windows.len()==1 && self.windows.first().is_some_and(|w|self.policy.manual_ready(w,self.now())),
            "awaiting_confirmation":self.awaiting_confirmation,
            "version":self.windows.first().map(|w|w.owner.version.as_str()),
            "elapsed_ms":self.running.as_ref().map(|r|r.started.elapsed().as_millis() as u64).unwrap_or(0),
            "detail":self.detail_text(),"core_pid":std::process::id(),
            "metrics":{"process_scans":self.discovery.process_scans,"window_scans":self.discovery.window_scans,"window_checks":self.discovery.window_checks,"ui_queries":self.ui_queries}
        })
    }
    fn light_state(&self) -> UiState {
        let (status, hint) = self
            .message
            .split_once(" · ")
            .unwrap_or((&self.message, ""));
        UiState {
            status: status.into(),
            hint: hint.into(),
            automatic_status: self.automatic_status().into(),
            automatic: self.settings.automatic,
            autostart: self.autostart,
            tray_only: self.settings.tray_only,
            busy: self.running.is_some(),
            can_repair: self.running.is_none()
                && self.windows.len() == 1
                && self.policy.manual_ready(&self.windows[0], self.now()),
            awaiting_confirmation: self.awaiting_confirmation,
            revision: self.revision,
            wait_remaining_ms: self.wait_until.unwrap_or(0).saturating_sub(self.now()),
        }
    }
    unsafe fn publish(&mut self) {
        let mut state = self.light_state();
        state.revision = 0;
        state.wait_remaining_ms = 0;
        if self.published.as_ref() != Some(&state) || self.published_wait != self.wait_until {
            self.revision = self.revision.wrapping_add(1);
            self.published = Some(state);
            self.published_wait = self.wait_until;
            if let Some(signal) = &self.state_signal {
                signal.set();
            }
        }
        if self.running.is_none() {
            let deadline = self
                .windows
                .first()
                .and_then(|w| self.policy.next_manual_change(w, self.now()));
            if let Some(deadline) = deadline {
                SetTimer(
                    self.hwnd,
                    7,
                    deadline.saturating_sub(self.now()).clamp(1, 300000) as u32,
                    None,
                );
            } else {
                KillTimer(self.hwnd, 7);
            }
        }
    }
    unsafe fn bridge_command(&mut self, request: bridge::Request) -> serde_json::Value {
        self.action_error = None;
        match request.command.as_str() {
            "status" => return self.snapshot(),
            "ui_status" => {
                self.ui_queries += 1;
            }
            "repair" => self.command(FIX),
            "cancel" => {
                if let Some(r) = &self.running {
                    r.cancel();
                    self.say("正在取消…");
                }
            }
            "automatic" => {
                if let Some(enabled) = request.enabled {
                    if self.settings.automatic != enabled {
                        self.command(AUTO);
                    }
                }
            }
            "autostart" => {
                if let Some(enabled) = request.enabled {
                    if storage::autostart_enabled() != enabled {
                        self.command(START);
                    }
                }
            }
            "tray_only" => {
                if let Some(enabled) = request.enabled {
                    if self.settings.tray_only != enabled {
                        self.command(TRAY_ONLY);
                    }
                }
            }
            "confirm" => {
                if self.awaiting_confirmation {
                    match request.enabled {
                        Some(true) => self.command(YES),
                        Some(false) => self.command(NO),
                        None => {}
                    }
                }
            }
            "open" => self.command(OPEN),
            "open_log" => {
                if let Err(e) = self.open_log() {
                    return serde_json::json!({"error":e});
                }
            }
            "copy_diagnostics" => {
                if let Err(e) = self.copy_diagnostics() {
                    return serde_json::json!({"error":e});
                }
            }
            "open_log_folder" => {
                if let Err(e) = self.open_log_folder() {
                    return serde_json::json!({"error":e});
                }
            }
            "exit" => {
                SetTimer(self.hwnd, 6, 100, None);
            }
            _ => return serde_json::json!({"error":"unknown_command"}),
        }
        self.publish();
        let mut reply = serde_json::to_value(self.light_state()).unwrap();
        reply["message"] = self.status_text().into();
        if let Some(error) = &self.action_error {
            reply["error"] = error.clone().into();
        }
        reply
    }
    unsafe fn open_ui(&mut self) {
        if let Some(child) = &mut self.ui_process {
            if child.try_wait().ok().flatten().is_none() {
                EnumWindows(Some(raise_ui_window), child.id() as isize);
                return;
            }
        }
        match std::process::Command::new(bridge::ui_path()).spawn() {
            Ok(child) => self.ui_process = Some(child),
            Err(e) => {
                self.say("无法打开界面，请检查 UI 组件");
                storage::log(&format!("UI launch: {e}"));
            }
        }
    }
    unsafe fn command(&mut self, id: u16) {
        self.action_error = None;
        if let Err(error) = self.perform_command(id) {
            self.say(&error);
            self.action_error = Some(error);
        }
        if !self.closing {
            self.publish();
        }
    }
    unsafe fn perform_command(&mut self, id: u16) -> Result<(), String> {
        match id {
            FIX => {
                if let Some(r) = &self.running {
                    r.cancel();
                    self.say("正在取消…");
                } else {
                    self.refresh(true);
                    self.try_fix(true);
                }
            }
            AUTO | TOGGLE_AUTO => {
                if self.running.is_some() {
                    return Err("修复进行中，完成后可调整自动修复".into());
                }
                self.settings.automatic = !self.settings.automatic;
                if let Err(error) = self.settings.save() {
                    self.settings.automatic = !self.settings.automatic;
                    return Err(error);
                }
                storage::event(
                    "SETTING_AUTOMATIC",
                    None,
                    "用户更改自动修复设置",
                    if self.settings.automatic {
                        "自动修复：开启"
                    } else {
                        "自动修复：关闭"
                    },
                );
                if !self.settings.automatic {
                    KillTimer(self.hwnd, 2);
                    self.say("自动修复已关闭");
                } else {
                    self.schedule_automatic();
                }
            }
            START => {
                self.autostart = storage::autostart_enabled();
                storage::autostart(!self.autostart)?;
                self.autostart = storage::autostart_enabled();
            }
            DETAIL => self.open_log()?,
            YES => {
                if let Some(w) = self.target.clone().filter(|_| {
                    self.awaiting_confirmation
                        && self.last.as_ref().is_some_and(|r| r.can_confirm())
                }) {
                    let versions = self.settings.verified_versions.clone();
                    self.settings.confirm_version(&w.owner.version);
                    if let Err(error) = self.settings.save() {
                        self.settings.verified_versions = versions;
                        return Err(error);
                    }
                    self.policy.complete(&w);
                    self.awaiting_confirmation = false;
                    storage::event(
                        "REPAIR_FEEDBACK",
                        Some(&self.last_operation),
                        "用户确认可以拖动",
                        "已记录此客户端版本；自动修复开关保持原值",
                    );
                    self.say("已确认可以拖动");
                }
            }
            NO => {
                if self.awaiting_confirmation {
                    storage::event(
                        "REPAIR_FEEDBACK",
                        Some(&self.last_operation),
                        "用户反馈仍无法拖动",
                        "自动处理暂停，开关偏好保留",
                    );
                    self.policy.halted = true;
                    self.awaiting_confirmation = false;
                    self.say("自动修复已暂停 · 需要时可以再次手动修复");
                }
            }
            OPEN => self.open_ui(),
            TRAY_ONLY => {
                self.settings.tray_only = !self.settings.tray_only;
                if let Err(error) = self.settings.save() {
                    self.settings.tray_only = !self.settings.tray_only;
                    return Err(error);
                }
            }
            COPY_DIAGNOSTICS => self.copy_diagnostics()?,
            OPEN_LOG_FOLDER => self.open_log_folder()?,
            EXIT => {
                if let Some(child) = &self.ui_process {
                    EnumWindows(Some(close_ui_window), child.id() as isize);
                }
                self.closing = true;
                if let Some(r) = &self.running {
                    r.cancel();
                    self.say("正在退出…");
                } else {
                    DestroyWindow(self.hwnd);
                }
            }
            _ => {}
        }
        Ok(())
    }
    unsafe fn menu(&mut self) {
        let m = CreatePopupMenu();
        let repair_menu = CreatePopupMenu();
        let settings_menu = CreatePopupMenu();
        AppendMenuW(
            repair_menu,
            MF_STRING | MF_GRAYED,
            0,
            wide(&self.status_text()).as_ptr(),
        );
        AppendMenuW(repair_menu, MF_SEPARATOR, 0, null());
        for (id, label) in [
            (
                FIX,
                if self.running.is_some() {
                    "取消修复"
                } else {
                    "立即修复"
                },
            ),
            (TOGGLE_AUTO, "自动修复"),
        ] {
            let checked = id == TOGGLE_AUTO && self.settings.automatic;
            AppendMenuW(
                repair_menu,
                MF_STRING
                    | if checked { MF_CHECKED } else { 0 }
                    | if (id == TOGGLE_AUTO && self.running.is_some())
                        || (id == FIX && self.running.is_none() && self.windows.len() != 1)
                    {
                        MF_GRAYED
                    } else {
                        0
                    },
                id as usize,
                wide(label).as_ptr(),
            );
        }
        if self.awaiting_confirmation {
            AppendMenuW(repair_menu, MF_SEPARATOR, 0, null());
            AppendMenuW(repair_menu, MF_STRING, YES as usize, w!("可以拖动"));
            AppendMenuW(repair_menu, MF_STRING, NO as usize, w!("仍无法拖动"));
        }
        for (id, label, checked) in [
            (OPEN, "打开界面", false),
            (TRAY_ONLY, "仅托盘启动", self.settings.tray_only),
            (START, "开机启动", storage::autostart_enabled()),
            (DETAIL, "打开日志", false),
            (COPY_DIAGNOSTICS, "复制诊断摘要", false),
            (OPEN_LOG_FOLDER, "打开日志文件夹", false),
        ] {
            AppendMenuW(
                settings_menu,
                MF_STRING | if checked { MF_CHECKED } else { 0 },
                id as usize,
                wide(label).as_ptr(),
            );
        }
        AppendMenuW(
            m,
            MF_POPUP,
            repair_menu as usize,
            if self.awaiting_confirmation {
                w!("修复（待确认）")
            } else {
                w!("修复")
            },
        );
        AppendMenuW(m, MF_POPUP, settings_menu as usize, w!("设置"));
        AppendMenuW(m, MF_STRING, EXIT as usize, w!("退出"));
        let mut p: POINT = zeroed();
        GetCursorPos(&mut p);
        SetForegroundWindow(self.hwnd);
        let id = TrackPopupMenu(
            m,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            p.x,
            p.y,
            0,
            self.hwnd,
            null(),
        );
        DestroyMenu(m);
        if id != 0 {
            self.command(id as u16);
        }
    }
}
fn copy_wide(out: &mut [u16], s: &str) {
    let len = out.len().saturating_sub(1);
    for (a, b) in out.iter_mut().take(len).zip(s.encode_utf16()) {
        *a = b;
    }
}
#[allow(clippy::manual_dangling_ptr)] // MAKEINTRESOURCEW(1), not an address to dereference.
unsafe fn create_icon() -> HICON {
    LoadImageW(
        GetModuleHandleW(null()),
        1usize as *const u16,
        IMAGE_ICON,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
        LR_DEFAULTCOLOR,
    ) as HICON
}
unsafe extern "system" fn event_callback(
    _: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    obj: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if obj != OBJID_WINDOW || child != 0 {
        return;
    }
    let root = ROOT.load(Ordering::Relaxed);
    if root != 0 {
        PostMessageW(root as HWND, EVENT, event as usize, hwnd as isize);
    }
}
unsafe extern "system" fn raise_ui_window(hwnd: HWND, pid: isize) -> i32 {
    let mut owner = 0;
    GetWindowThreadProcessId(hwnd, &mut owner);
    if owner == pid as u32 && IsWindowVisible(hwnd) != 0 {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        return 0;
    }
    1
}
unsafe extern "system" fn close_ui_window(hwnd: HWND, pid: isize) -> i32 {
    let mut owner = 0;
    GetWindowThreadProcessId(hwnd, &mut owner);
    if owner == pid as u32 && IsWindowVisible(hwnd) != 0 {
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
    1
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == WM_NCCREATE {
        let c = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, c.lpCreateParams as isize);
        (*(c.lpCreateParams as *mut App)).hwnd = hwnd;
        ROOT.store(hwnd as usize, Ordering::Relaxed);
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if p.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let app = &mut *p;
    if msg == app.taskbar && app.taskbar != 0 {
        app.add_tray();
        return 0;
    }
    match msg {
        bridge::MESSAGE => {
            let delivery = Box::from_raw(lp as *mut bridge::Delivery);
            let result = app.bridge_command(delivery.request);
            let _ = delivery.reply.send(result);
        }
        WM_CREATE => app.init(),
        WM_COMMAND => app.command((wp & 0xffff) as u16),
        WM_TIMER => match wp {
            1 => {
                if let Some(child) = &mut app.ui_process {
                    if child.try_wait().ok().flatten().is_some() {
                        app.ui_process = None;
                    }
                }
                app.autostart = storage::autostart_enabled();
                if app.discovery.owners.is_empty() {
                    app.refresh(true);
                    app.schedule_automatic();
                }
            }
            2 => {
                KillTimer(hwnd, 2);
                app.refresh(false);
                app.schedule_automatic();
            }
            3 => app.poll_result(),
            6 => {
                KillTimer(hwnd, 6);
                app.command(EXIT);
            }
            7 => {
                KillTimer(hwnd, 7);
                if app.wait_until.is_some_and(|deadline| deadline <= app.now())
                    && !app.awaiting_confirmation
                {
                    app.refresh(false);
                    if app.windows.len() == 1 {
                        app.say("等待已结束 · 可以点击“立即修复”");
                    }
                    app.schedule_automatic();
                }
            }
            8 => {
                KillTimer(hwnd, 8);
                app.events_due = None;
                app.refresh(false);
                app.schedule_automatic();
            }
            9 => {
                if !app.discovery.owners.is_empty() {
                    app.refresh(true);
                    app.schedule_automatic();
                }
            }
            _ => {}
        },
        EVENT => {
            let target = lp as usize;
            if target != 0 {
                app.dirty.insert(target);
                if wp as u32 == EVENT_OBJECT_DESTROY
                    || (wp as u32 == EVENT_OBJECT_HIDE && app.running.is_none())
                {
                    app.policy.forget_hwnd(target);
                }
                let now = app.now();
                if arm_event_batch(&mut app.events_due, now) {
                    SetTimer(hwnd, 8, 300, None);
                }
            }
        }
        PROCESS_EXIT => {
            app.refresh(true);
            app.schedule_automatic();
        }
        TRAY => match (lp as u32) & 0xffff {
            WM_CONTEXTMENU => app.menu(),
            NIN_SELECT | NIN_KEYSELECT => app.command(OPEN),
            _ => {}
        },
        WM_DISPLAYCHANGE | WM_POWERBROADCAST => {
            app.reconcile_pending = true;
            SetTimer(hwnd, 2, 2100, None);
        }
        WM_CLOSE => app.command(EXIT),
        WM_DESTROY => {
            ROOT.store(0, Ordering::Relaxed);
            for hook in app.hooks.drain(..) {
                UnhookWinEvent(hook);
            }
            app.process_watches.clear();
            if app.tray {
                Shell_NotifyIconW(NIM_DELETE, &app.tray_data());
            }
            if let Some(r) = &app.running {
                r.cancel();
            }
            DestroyIcon(app.icon);
            PostQuitMessage(0);
            return 0;
        }
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    if !app.closing {
        app.publish();
    }
    0
}
pub fn run(tray: bool, force_ui: bool) {
    unsafe {
        let _gate = match native::Gate::acquire("Local\\CodexPetRepairGuiV1") {
            Ok(gate) => gate,
            Err(_) => {
                let hwnd = FindWindowW(w!("PetRepair.Main"), null());
                if !hwnd.is_null() && !tray {
                    PostMessageW(hwnd, WM_COMMAND, OPEN as usize, 0);
                }
                return;
            }
        };
        let instance = GetModuleHandleW(null());
        let mut class: WNDCLASSW = zeroed();
        class.lpfnWndProc = Some(proc);
        class.hInstance = instance;
        class.lpszClassName = w!("PetRepair.Main");
        RegisterClassW(&class);
        let mut app = Box::new(App::new());
        let hwnd = CreateWindowExW(
            0,
            w!("PetRepair.Main"),
            w!("Codex 宠物修复"),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            app.as_mut() as *mut _ as _,
        );
        if hwnd.is_null() {
            return;
        }
        if !tray && (force_ui || !app.settings.tray_only) {
            app.open_ui();
        }
        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_storm_keeps_first_deadline() {
        let mut due = None;
        assert!(arm_event_batch(&mut due, 1000));
        for time in 1001..5000 {
            assert!(!arm_event_batch(&mut due, time));
        }
        assert_eq!(due, Some(1300));
        due = None;
        assert!(arm_event_batch(&mut due, 5000));
        assert_eq!(due, Some(5300));
    }
    #[test]
    fn busy_repair_defers_events_and_full_reconciliation() {
        let mut app = App::new();
        let (_tx, rx) = std::sync::mpsc::channel();
        app.running = Some(repair::Running {
            operation_id: "test".into(),
            input: std::sync::Arc::new(std::sync::Mutex::new(None)),
            result: rx,
            started: Instant::now(),
            worker_pid: 0,
        });
        app.dirty.extend([10, 20]);
        unsafe {
            app.refresh(true);
        }
        assert!(app.reconcile_pending);
        assert_eq!(app.dirty.len(), 2);
        assert_eq!(app.discovery.process_scans, 0);
        assert_eq!(app.discovery.window_scans, 0);
    }
}
