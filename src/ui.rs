use crate::{
    appearance, bridge,
    native::{self, wide, Window},
    policy::Policy,
    repair,
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
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{
            Accessibility::*,
            Controls::*,
            HiDpi::*,
            Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus},
            Shell::*,
            WindowsAndMessaging::*,
        },
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
struct App {
    hwnd: HWND,
    controls: Vec<(HWND, [i32; 4])>,
    font: HFONT,
    heading: HFONT,
    status: HWND,
    button: HWND,
    auto: HWND,
    start: HWND,
    yes: HWND,
    no: HWND,
    preview: Option<std::path::PathBuf>,
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
    last_scan: u64,
    ignore_events_until: u64,
    icon: HICON,
    process_watches: Vec<native::ProcessWatch>,
    focus: HWND,
    awaiting_confirmation: bool,
    motions: std::collections::HashMap<usize, appearance::Motion>,
    animations: bool,
    headless: bool,
    ui_process: Option<std::process::Child>,
    wait_until: Option<u64>,
    last_operation: String,
    last_error: String,
}
impl App {
    fn new() -> Self {
        Self {
            hwnd: null_mut(),
            controls: Vec::new(),
            font: null_mut(),
            heading: null_mut(),
            status: null_mut(),
            button: null_mut(),
            auto: null_mut(),
            start: null_mut(),
            yes: null_mut(),
            no: null_mut(),
            preview: None,
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
            last_scan: 0,
            ignore_events_until: 0,
            icon: null_mut(),
            process_watches: Vec::new(),
            focus: null_mut(),
            awaiting_confirmation: false,
            motions: Default::default(),
            animations: false,
            headless: false,
            ui_process: None,
            wait_until: None,
            last_operation: String::new(),
            last_error: String::new(),
        }
    }
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }
    unsafe fn child(
        &mut self,
        class: *const u16,
        text: &str,
        style: u32,
        id: u16,
        r: [i32; 4],
    ) -> HWND {
        let h = CreateWindowExW(
            0,
            class,
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            1,
            1,
            self.hwnd,
            id as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        self.controls.push((h, r));
        h
    }
    unsafe fn init(&mut self) {
        self.animations = !self.headless && appearance::animations_enabled();
        self.icon = create_icon();
        SendMessageW(
            self.hwnd,
            WM_SETICON,
            ICON_SMALL as usize,
            self.icon as isize,
        );
        SendMessageW(self.hwnd, WM_SETICON, ICON_BIG as usize, self.icon as isize);
        if !self.headless {
            appearance::frame(self.hwnd);
            self.child(w!("STATIC"), "Codex 宠物修复", 0, 0, [24, 22, 276, 34]);
            self.status = self.child(w!("STATIC"), "正在检测…", 0, 0, [44, 92, 312, 40]);
            self.button = self.child(
                w!("BUTTON"),
                "立即修复",
                WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                FIX,
                [44, 144, 312, 40],
            );
            self.auto = self.child(
                w!("BUTTON"),
                "自动修复",
                WS_TABSTOP | BS_CHECKBOX as u32,
                AUTO,
                [24, 220, 352, 48],
            );
            self.start = self.child(
                w!("BUTTON"),
                "开机启动",
                WS_TABSTOP | BS_CHECKBOX as u32,
                START,
                [24, 272, 352, 48],
            );
            self.child(w!("BUTTON"), "详情", WS_TABSTOP, DETAIL, [312, 22, 64, 32]);
            self.yes = self.child(
                w!("BUTTON"),
                "可以拖动",
                WS_TABSTOP,
                YES,
                [44, 144, 150, 40],
            );
            self.no = self.child(
                w!("BUTTON"),
                "仍无法拖动",
                WS_TABSTOP,
                NO,
                [206, 144, 150, 40],
            );
            SendMessageW(
                self.auto,
                BM_SETCHECK,
                if self.settings.automatic {
                    BST_CHECKED
                } else {
                    BST_UNCHECKED
                } as usize,
                0,
            );
            SendMessageW(
                self.start,
                BM_SETCHECK,
                if storage::autostart_enabled() {
                    BST_CHECKED
                } else {
                    BST_UNCHECKED
                } as usize,
                0,
            );
            self.feedback(false);
            self.layout();
        }
        self.taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
        if self.preview.is_none() {
            self.add_tray();
        }
        if self.preview.is_none() {
            if let Some(r) = repair::recover_pending() {
                storage::log(&format!("startup recovery {:?}", r));
                self.last = Some(r);
            }
        }
        self.refresh(true);
        if self.headless {
            if let Err(e) = bridge::listen(self.hwnd as usize) {
                self.say(&e);
            }
        }
        SetTimer(self.hwnd, 1, 10000, None);
        if self.preview.is_some() {
            SetTimer(self.hwnd, 4, 500, None);
        }
    }
    unsafe fn layout(&mut self) {
        let dpi = GetDpiForWindow(self.hwnd).max(96);
        let scale = |v: i32| v * dpi as i32 / 96;
        let old = self.font;
        let old_h = self.heading;
        self.font = CreateFontW(
            -scale(15),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            w!("Microsoft YaHei UI"),
        );
        self.heading = CreateFontW(
            -scale(22),
            0,
            0,
            0,
            600,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            w!("Microsoft YaHei UI"),
        );
        for (i, (h, r)) in self.controls.iter().enumerate() {
            MoveWindow(*h, scale(r[0]), scale(r[1]), scale(r[2]), scale(r[3]), 1);
            SendMessageW(
                *h,
                WM_SETFONT,
                if i == 0 { self.heading } else { self.font } as usize,
                1,
            );
        }
        if !old.is_null() {
            DeleteObject(old);
        }
        if !old_h.is_null() {
            DeleteObject(old_h);
        }
        InvalidateRect(self.hwnd, null(), 1);
    }
    unsafe fn feedback(&self, visible: bool) {
        ShowWindow(self.button, if visible { SW_HIDE } else { SW_SHOW });
        ShowWindow(self.yes, if visible { SW_SHOW } else { SW_HIDE });
        ShowWindow(self.no, if visible { SW_SHOW } else { SW_HIDE });
    }
    unsafe fn paint(&self, dc: HDC) {
        appearance::background(dc, self.hwnd);
        if let Some(r) = &self.running {
            appearance::progress(
                dc,
                self.hwnd,
                (r.started.elapsed().as_secs_f32() / 3.0).min(0.98),
            );
        }
    }
    unsafe fn invalidate_progress(&self) {
        let s = |v: i32| v * GetDpiForWindow(self.hwnd).max(96) as i32 / 96;
        let r = RECT {
            left: s(44),
            top: s(134),
            right: s(356),
            bottom: s(138),
        };
        InvalidateRect(self.hwnd, &r, 0);
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
            SetWindowTextW(self.status, wide(message).as_ptr());
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
            if let Some(w) = native::ProcessWatch::start(*pid, self.hwnd as usize, PROCESS_EXIT) {
                self.process_watches.push(w);
            }
        }
        self.pids = pids;
    }
    unsafe fn refresh(&mut self, processes: bool) {
        if self.running.is_some() {
            return;
        }
        if processes {
            self.discovery.refresh_processes();
            self.sync_hooks();
        }
        self.windows = self.discovery.scan();
        let now = self.now();
        self.policy.observe(&self.windows, now);
        self.last_scan = now;
        EnableWindow(self.button, (self.windows.len() == 1) as i32);
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
        } else if self.settings.automatic && self.preview.is_none() {
            self.try_fix(false);
        } else if self.last.is_none() {
            self.say("可以修复 · 点击“立即修复”");
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
                self.feedback(false);
                EnableWindow(self.button, 1);
                SetWindowTextW(self.button, w!("取消"));
                EnableWindow(self.auto, 0);
                self.say("正在修复 · 约需 3 秒，请暂时不要拖动宠物");
                SetTimer(self.hwnd, 3, 100, None);
                if self.animations {
                    SetTimer(self.hwnd, 5, 16, None);
                }
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
            self.invalidate_progress();
            KillTimer(self.hwnd, 3);
            EnableWindow(self.button, 1);
            EnableWindow(self.auto, 1);
            SetWindowTextW(self.button, w!("立即修复"));
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
                    self.feedback(true);
                    SetFocus(self.yes);
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
            self.ignore_events_until = self.now() + 2000;
            if self.closing {
                DestroyWindow(self.hwnd);
            }
        } else if r.started.elapsed().as_secs() > 12 {
            r.cancel();
            self.say("修复超时，正在恢复原状");
        }
    }
    unsafe fn details(&self) {
        MessageBoxW(
            self.hwnd,
            wide(&self.detail_text()).as_ptr(),
            w!("恢复详情"),
            MB_OK | MB_ICONINFORMATION,
        );
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
            "autostart":storage::autostart_enabled(),"tray_only":self.settings.tray_only,
            "busy":self.running.is_some(),"can_repair":self.windows.len()==1 && self.windows.first().is_some_and(|w|self.policy.manual_ready(w,self.now())),
            "awaiting_confirmation":self.awaiting_confirmation,
            "version":self.windows.first().map(|w|w.owner.version.as_str()),
            "elapsed_ms":self.running.as_ref().map(|r|r.started.elapsed().as_millis() as u64).unwrap_or(0),
            "detail":self.detail_text(),"core_pid":std::process::id()
        })
    }
    unsafe fn bridge_command(&mut self, request: bridge::Request) -> serde_json::Value {
        match request.command.as_str() {
            "status" => {}
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
        self.snapshot()
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
        match id {
            FIX => {
                if let Some(r) = &self.running {
                    r.cancel();
                    self.say("正在取消…");
                    return;
                }
                self.refresh(true);
                self.try_fix(true);
            }
            AUTO => {
                if self.running.is_some() {
                    return;
                }
                let enabled = !self.settings.automatic;
                self.settings.automatic = enabled;
                if let Err(e) = self.settings.save() {
                    self.settings.automatic = !enabled;
                    SendMessageW(
                        self.auto,
                        BM_SETCHECK,
                        if !enabled { BST_CHECKED } else { BST_UNCHECKED } as usize,
                        0,
                    );
                    self.say(&e);
                } else {
                    SendMessageW(
                        self.auto,
                        BM_SETCHECK,
                        if enabled { BST_CHECKED } else { BST_UNCHECKED } as usize,
                        0,
                    );
                    storage::event(
                        "SETTING_AUTOMATIC",
                        None,
                        "用户更改自动修复设置",
                        if enabled {
                            "自动修复：开启"
                        } else {
                            "自动修复：关闭"
                        },
                    );
                    self.say(if enabled && self.policy.halted {
                        "自动修复已暂停，请先手动修复"
                    } else if enabled {
                        "自动修复已开启"
                    } else {
                        "自动修复已关闭"
                    });
                    self.refresh(true);
                }
            }
            START => {
                let enabled = !storage::autostart_enabled();
                if let Err(e) = storage::autostart(enabled) {
                    SendMessageW(
                        self.start,
                        BM_SETCHECK,
                        if !enabled { BST_CHECKED } else { BST_UNCHECKED } as usize,
                        0,
                    );
                    self.say(&e);
                } else {
                    SendMessageW(
                        self.start,
                        BM_SETCHECK,
                        if enabled { BST_CHECKED } else { BST_UNCHECKED } as usize,
                        0,
                    );
                }
            }
            DETAIL => {
                if self.headless {
                    if let Err(e) = self.open_log() {
                        self.say(&e);
                    }
                } else {
                    self.details();
                }
            }
            YES => {
                if let Some(w) = &self.target {
                    if self.last.as_ref().is_some_and(|r| r.can_confirm()) {
                        let previous_versions = self.settings.verified_versions.clone();
                        self.settings.confirm_version(&w.owner.version);
                        if let Err(e) = self.settings.save() {
                            self.settings.verified_versions = previous_versions;
                            self.say(&e);
                        } else {
                            self.policy.complete(w);
                            self.awaiting_confirmation = false;
                            storage::event(
                                "REPAIR_FEEDBACK",
                                Some(&self.last_operation),
                                "用户确认可以拖动",
                                "已记录此客户端版本；自动修复开关保持原值",
                            );
                            self.say("已确认可以拖动");
                            self.feedback(false);
                            SetFocus(self.button);
                        }
                    }
                }
            }
            NO => {
                storage::event(
                    "REPAIR_FEEDBACK",
                    Some(&self.last_operation),
                    "用户反馈仍无法拖动",
                    "自动处理暂停，开关偏好保留",
                );
                self.policy.halted = true;
                self.awaiting_confirmation = false;
                self.say("自动修复已暂停");
                self.feedback(false);
                SetFocus(self.button);
            }
            OPEN => {
                if self.headless {
                    self.open_ui();
                } else {
                    ShowWindow(self.hwnd, SW_SHOW);
                    SetForegroundWindow(self.hwnd);
                }
            }
            TRAY_ONLY => {
                let previous = self.settings.tray_only;
                self.settings.tray_only = !previous;
                if let Err(e) = self.settings.save() {
                    self.settings.tray_only = previous;
                    self.say(&e);
                }
            }
            COPY_DIAGNOSTICS => {
                if let Err(e) = self.copy_diagnostics() {
                    self.say(&e);
                }
            }
            OPEN_LOG_FOLDER => {
                if let Err(e) = self.open_log_folder() {
                    self.say(&e);
                }
            }
            TOGGLE_AUTO => {
                if self.running.is_none() {
                    self.command(AUTO);
                }
            }
            EXIT => {
                if self.headless {
                    if let Some(child) = &self.ui_process {
                        EnumWindows(Some(close_ui_window), child.id() as isize);
                    }
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
            0
        }
        WM_CREATE => {
            app.init();
            0
        }
        WM_COMMAND => {
            if (wp >> 16) == BN_CLICKED as usize {
                app.command((wp & 0xffff) as u16);
            }
            0
        }
        WM_SETFOCUS => {
            if !app.focus.is_null() && IsWindowVisible(app.focus) != 0 {
                SetFocus(app.focus);
            } else if !app.button.is_null() {
                SetFocus(app.button);
            }
            0
        }
        WM_ACTIVATE => {
            if wp & 0xffff == WA_INACTIVE as usize {
                app.focus = GetFocus();
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_NOTIFY => {
            let header = &*(lp as *const NMHDR);
            if header.code == NM_CUSTOMDRAW
                && matches!(header.idFrom as u16, FIX | AUTO | START | DETAIL | YES | NO)
            {
                let draw = &*(lp as *const NMCUSTOMDRAW);
                if draw.dwDrawStage != CDDS_PREPAINT {
                    return CDRF_DODEFAULT as isize;
                }
                let target = appearance::control_state(draw);
                let now = app.now();
                let animate = app.animations && IsWindowVisible(hwnd) != 0;
                let motion = app
                    .motions
                    .entry(header.hwndFrom as usize)
                    .or_insert_with(|| appearance::Motion::new(target));
                let values = motion.update(target, now, animate);
                if motion.active(now) {
                    SetTimer(hwnd, 5, 16, None);
                }
                appearance::button(
                    draw,
                    app.font,
                    header.idFrom == FIX as usize,
                    matches!(header.idFrom as u16, AUTO | START),
                    values,
                )
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_ERASEBKGND => {
            app.paint(wp as HDC);
            1
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            app.paint(dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_PRINTCLIENT => {
            app.paint(wp as HDC);
            0
        }
        WM_CTLCOLORSTATIC => appearance::static_color(wp as HDC, lp as HWND == app.status),
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            app.animations = !app.headless && appearance::animations_enabled();
            appearance::frame(hwnd);
            RedrawWindow(
                hwnd,
                null(),
                null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
            );
            0
        }
        WM_TIMER => {
            match wp {
                1 => {
                    if let Some(child) = &mut app.ui_process {
                        if let Ok(Some(status)) = child.try_wait() {
                            if !status.success() {
                                storage::log(&format!("UI exited: {status}"));
                            }
                            app.ui_process = None;
                        }
                    }
                    if app.discovery.owners.is_empty()
                        || app.now().saturating_sub(app.last_scan) >= 30000
                    {
                        app.refresh(true);
                    }
                }
                2 => {
                    KillTimer(hwnd, 2);
                    app.refresh(false);
                }
                3 => {
                    app.poll_result();
                    app.invalidate_progress();
                }
                5 => {
                    let now = app.now();
                    for control in app.motions.keys() {
                        InvalidateRect(*control as HWND, null(), 0);
                    }
                    app.invalidate_progress();
                    if !app.animations
                        || IsWindowVisible(hwnd) == 0
                        || (app.running.is_none() && !app.motions.values().any(|m| m.active(now)))
                    {
                        KillTimer(hwnd, 5);
                    }
                }
                6 => {
                    KillTimer(hwnd, 6);
                    app.command(EXIT);
                }
                4 => {
                    KillTimer(hwnd, 4);
                    if let Some(path) = &app.preview {
                        if let Err(e) = appearance::preview(hwnd, path) {
                            storage::log(&format!("UI preview: {e}"));
                        }
                        app.command(EXIT);
                    }
                }
                _ => {}
            }
            0
        }
        EVENT => {
            if app.running.is_none() && app.now() >= app.ignore_events_until {
                if wp as u32 == EVENT_OBJECT_HIDE || wp as u32 == EVENT_OBJECT_DESTROY {
                    app.policy.forget_hwnd(lp as usize);
                }
                SetTimer(hwnd, 2, 300, None);
            }
            0
        }
        PROCESS_EXIT => {
            app.refresh(true);
            0
        }
        TRAY => {
            match (lp as u32) & 0xffff {
                WM_CONTEXTMENU => app.menu(),
                NIN_SELECT | NIN_KEYSELECT => app.command(OPEN),
                _ => {}
            }
            0
        }
        WM_DPICHANGED => {
            let r = &*(lp as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            app.layout();
            0
        }
        WM_DISPLAYCHANGE | WM_POWERBROADCAST => {
            SetTimer(hwnd, 2, 2100, None);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CLOSE => {
            if app.tray {
                ShowWindow(hwnd, SW_HIDE);
            } else {
                app.command(EXIT);
            }
            0
        }
        WM_DESTROY => {
            ROOT.store(0, Ordering::Relaxed);
            for h in app.hooks.drain(..) {
                UnhookWinEvent(h);
            }
            app.process_watches.clear();
            if app.tray {
                Shell_NotifyIconW(NIM_DELETE, &app.tray_data());
            }
            if let Some(r) = &app.running {
                r.cancel();
            }
            DeleteObject(app.font);
            DeleteObject(app.heading);
            DestroyIcon(app.icon);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub fn run(tray: bool, preview: Option<std::path::PathBuf>, legacy: bool, force_ui: bool) {
    unsafe {
        let _gate = match native::Gate::acquire("Local\\CodexPetRepairGuiV1") {
            Ok(g) => g,
            Err(_) => {
                let h = FindWindowW(w!("PetRepair.Main"), null());
                if !h.is_null() && !tray {
                    PostMessageW(h, WM_COMMAND, OPEN as usize, 0);
                }
                return;
            }
        };
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = instance;
        wc.lpszClassName = w!("PetRepair.Main");
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.hbrBackground = (COLOR_WINDOW + 1) as usize as HBRUSH;
        RegisterClassW(&wc);
        let mut app = Box::new(App::new());
        app.headless = preview.is_none() && !legacy;
        app.preview = preview;
        let dpi = GetDpiForSystem().max(96);
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 400 * dpi as i32 / 96,
            bottom: 344 * dpi as i32 / 96,
        };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        AdjustWindowRectExForDpi(&mut r, style, 0, 0, dpi);
        let hwnd = CreateWindowExW(
            0,
            w!("PetRepair.Main"),
            w!("Codex 宠物修复"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            r.right - r.left,
            r.bottom - r.top,
            null_mut(),
            null_mut(),
            instance,
            app.as_mut() as *mut _ as _,
        );
        if hwnd.is_null() {
            return;
        }
        if !tray {
            if app.headless {
                if force_ui || (!app.settings.tray_only && bridge::ui_path().is_file()) {
                    app.open_ui();
                }
            } else {
                ShowWindow(hwnd, SW_SHOW);
                SetFocus(app.button);
            }
        }
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
