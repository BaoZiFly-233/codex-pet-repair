use crate::{
    native::{self, Gate, Handle, Window},
    policy::restore_value,
    storage,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::windows::process::CommandExt,
    process::{ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_LAYERED;
pub const STYLE_CHANGED: &str = "检测到客户端并发改变样式，已保留其变化";

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Transaction {
    pub window: Window,
    pub original: u32,
    pub marker: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Report {
    pub code: String,
    pub restored: bool,
    pub exact: bool,
    pub elapsed_ms: u64,
}
impl Report {
    pub fn description(&self) -> (&'static str, &'static str, &'static str) {
        match self.code.as_str() {
            "reset" => (
                "REPAIR_COMPLETED",
                "窗口状态已重置",
                "请实际拖动宠物检查效果",
            ),
            STYLE_CHANGED => (
                "REPAIR_STYLE_MERGED",
                "窗口状态已恢复，并保留客户端的同时更新",
                "请实际拖动宠物检查效果",
            ),
            "cancelled_restored" => (
                "REPAIR_CANCELLED",
                "修复已取消，窗口原状态已恢复",
                "需要时可以再次手动修复",
            ),
            "input_interrupted_restored" => (
                "REPAIR_INPUT_INTERRUPTED",
                "桌面已锁定或切换到安全桌面，窗口原状态已恢复",
                "回到正常桌面后可以重新修复",
            ),
            "辅助进程中断，兜底恢复已完成" => (
                "REPAIR_RECOVERED",
                "修复辅助程序退出，窗口原状态已由后台恢复",
                "请检查宠物状态；若仍无效，请复制诊断摘要",
            ),
            "目标窗口已销毁或改变" => (
                "REPAIR_TARGET_CHANGED",
                "目标浮窗已关闭或重新创建，本次修复结束",
                "重新显示宠物后再试",
            ),
            _ => (
                "REPAIR_FAILED",
                "本次修复未能正常完成",
                "请复制诊断摘要，保留下面的原始错误信息",
            ),
        }
    }
    pub fn can_confirm(&self) -> bool {
        self.restored && (self.code == "reset" || self.code == STYLE_CHANGED)
    }
    fn fail(s: impl Into<String>) -> Self {
        Self {
            code: s.into(),
            restored: false,
            exact: false,
            elapsed_ms: 0,
        }
    }
}
fn lock(w: &Window) -> &'static str {
    if w.owner.test {
        "Local\\PetRepairSyntheticTests"
    } else {
        native::REPAIR_LOCK
    }
}
fn input_ready(w: &Window) -> bool {
    native::desktop() && (w.owner.test || !native::pointer_pressed_in(w))
}
#[derive(Serialize, Deserialize)]
struct Request {
    window: Window,
    parent: u32,
    parent_start: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "event")]
enum Event {
    Prepared { transaction: Transaction },
    Done { report: Report },
}
pub fn restore(t: &Transaction) -> Report {
    if !native::same(&t.window) || !native::marker_matches(t.window.hwnd, &t.marker) {
        return Report::fail("目标窗口已销毁或改变");
    }
    let Ok(current) = native::style(t.window.hwnd) else {
        return Report::fail("无法读取待恢复样式");
    };
    let (value, conflict) = restore_value(t.original, current);
    let result =
        native::set_style(t.window.hwnd, value).and_then(|_| native::refresh(t.window.hwnd));
    let actual = native::style(t.window.hwnd).ok();
    let restored = actual == Some(value);
    if restored {
        native::marker_remove(t.window.hwnd, &t.marker);
    }
    Report {
        code: if let Err(e) = result {
            e
        } else if !restored {
            "恢复核验失败".into()
        } else if conflict {
            STYLE_CHANGED.into()
        } else {
            "reset".into()
        },
        restored,
        exact: restored && !conflict,
        elapsed_ms: 0,
    }
}
struct RestoreGuard {
    transaction: Transaction,
    armed: bool,
}
impl Drop for RestoreGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = restore(&self.transaction);
        }
    }
}
fn emit(event: Event) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", serde_json::to_string(&event).unwrap());
    let _ = out.flush();
}
pub fn worker() {
    match worker_inner() {
        Ok(r) => emit(Event::Done { report: r }),
        Err(e) => emit(Event::Done {
            report: Report::fail(e),
        }),
    }
}
fn worker_inner() -> Result<Report, String> {
    let start = Instant::now();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if line.len() > 16384 {
        return Err("invalid_request".into());
    }
    let request: Request = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    let parent = Handle::process(request.parent)?;
    if native::process_start(parent.0)? != request.parent_start || !parent.alive() {
        return Err("parent_changed".into());
    }
    let _gate = Gate::acquire(lock(&request.window))?;
    if !request.window.owner.test && native::community_running() {
        return Err("other_repair_running".into());
    }
    if !native::same(&request.window) || !input_ready(&request.window) {
        return Err("目标或输入状态改变，请稍后重试".into());
    }
    // Re-enumerate to validate class and visibility immediately before mutation.
    let current = if request.window.owner.test {
        native::scan_test(request.window.owner.pid)
    } else {
        let mut d = native::Discovery::default();
        d.refresh_processes();
        d.scan()
    };
    if !current.iter().any(|w| w.key() == request.window.key()) {
        return Err("window_changed".into());
    }
    let original = native::style(request.window.hwnd)?;
    let marker = format!(
        "PetRepair-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    native::marker_set(request.window.hwnd, &marker)?;
    let transaction = Transaction {
        window: request.window,
        original,
        marker,
    };
    let mut guard = RestoreGuard {
        transaction: transaction.clone(),
        armed: true,
    };
    emit(Event::Prepared {
        transaction: transaction.clone(),
    });
    line.clear();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if line.trim() != "go" || !parent.alive() {
        return Err("cancelled_before_change".into());
    }
    if !native::same(&transaction.window) || !input_ready(&transaction.window) {
        return Err("准备期间目标或输入状态改变".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        c.store(true, Ordering::Release);
    });
    native::set_style(transaction.window.hwnd, original & !WS_EX_LAYERED)?;
    native::refresh(transaction.window.hwnd)?;
    let began = Instant::now();
    let mut interrupted = None;
    while began.elapsed() < Duration::from_secs(3) {
        interrupted = if cancel.load(Ordering::Acquire) || !parent.alive() {
            Some("cancelled_restored")
        // Ordinary clicks and typing must not interrupt a prepared transaction.
        // Only losing the interactive desktop requires immediate restoration.
        } else if !native::desktop() {
            Some("input_interrupted_restored")
        } else if !native::same(&transaction.window) {
            Some("target_changed")
        } else {
            None
        };
        if interrupted.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let mut report = restore(&transaction);
    guard.armed = !report.restored;
    if report.restored {
        if let Some(reason) = interrupted {
            report.code = reason.into();
        }
    }
    report.elapsed_ms = start.elapsed().as_millis() as u64;
    Ok(report)
}

pub struct Running {
    pub operation_id: String,
    pub input: Arc<Mutex<Option<ChildStdin>>>,
    pub result: mpsc::Receiver<Report>,
    pub started: Instant,
    pub worker_pid: u32,
}
impl Running {
    pub fn cancel(&self) {
        if let Ok(mut p) = self.input.lock() {
            if let Some(mut p) = p.take() {
                let _ = writeln!(p, "cancel");
            }
        }
    }
}
pub fn launch(window: Window) -> Result<Running, String> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let operation_id = format!(
        "{}-{:04}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .arg("--repair-worker")
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let worker_pid = child.id();
    storage::event(
        "REPAIR_STARTED",
        Some(&operation_id),
        "开始修复",
        &format!(
            "客户端版本：{}\n目标进程：{}\n目标窗口：{}\n预计用时：约 3 秒",
            window.owner.version, window.owner.pid, window.hwnd
        ),
    );
    let parent = Handle::process(std::process::id())?;
    let request = Request {
        window,
        parent: std::process::id(),
        parent_start: native::process_start(parent.0)?,
    };
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{}", serde_json::to_string(&request).unwrap()).map_err(|e| e.to_string())?;
    let input = Arc::new(Mutex::new(Some(input)));
    let writer = input.clone();
    let output = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let errors = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.take(4096).read_to_string(&mut text);
        text
    });
    let (tx, rx) = mpsc::channel();
    let log_id = operation_id.clone();
    thread::spawn(move || {
        let mut prepared: Option<Transaction> = None;
        let mut report = None;
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else {
                break;
            };
            if line.len() > 32768 {
                break;
            }
            match serde_json::from_str::<Event>(&line) {
                Ok(Event::Prepared { transaction }) => {
                    let saved = storage::write_json(
                        &storage::data_dir().join("pending.json"),
                        &transaction,
                    );
                    prepared = Some(transaction);
                    if let Ok(mut writer) = writer.lock() {
                        if let Some(w) = writer.as_mut() {
                            let _ = writeln!(w, "{}", if saved.is_ok() { "go" } else { "cancel" });
                        }
                    }
                }
                Ok(Event::Done { report: r }) => {
                    report = Some(r);
                }
                Err(e) => {
                    storage::event(
                        "REPAIR_PROTOCOL_FAILED",
                        Some(&log_id),
                        "修复辅助程序返回了无法识别的信息",
                        &format!("原始错误：{e}\n原始响应：{line}"),
                    );
                    break;
                }
            }
        }
        if let Ok(mut w) = writer.lock() {
            w.take();
        }
        let status = child.wait();
        let errors = errors.join().unwrap_or_default();
        let mut result = report
            .unwrap_or_else(|| Report::fail(format!("辅助进程异常退出: {status:?} {errors}")));
        if !result.restored {
            if let Some(t) = prepared.as_ref() {
                // Worker has exited before fallback starts; never race two restorers.
                if let Ok(_gate) = Gate::acquire(lock(&t.window)) {
                    if native::marker_matches(t.window.hwnd, &t.marker) {
                        let fallback = restore(t);
                        if fallback.restored {
                            result = Report {
                                code: "辅助进程中断，兜底恢复已完成".into(),
                                ..fallback
                            };
                        }
                    }
                }
            }
        }
        if result.restored || prepared.as_ref().is_some_and(|t| !native::same(&t.window)) {
            let _ = std::fs::remove_file(storage::data_dir().join("pending.json"));
        }
        let (code, message, advice) = result.description();
        storage::event(code,Some(&log_id),message,&format!("建议：{advice}\n窗口原状态已恢复：{}\n完整原样式：{}\n耗时：{:.2} 秒\n原始结果：{}",if result.restored {"是"} else {"未确认"},if result.exact {"是"} else {"否；请结合结果说明"},result.elapsed_ms as f64/1000.0,result.code));
        let _ = tx.send(result);
    });
    Ok(Running {
        operation_id,
        input,
        result: rx,
        started: Instant::now(),
        worker_pid,
    })
}
pub fn recover_pending() -> Option<Report> {
    let p = storage::data_dir().join("pending.json");
    let bytes = std::fs::read(&p).ok()?;
    let t: Transaction = match serde_json::from_slice(&bytes) {
        Ok(t) => t,
        Err(_) => return Some(Report::fail("未完成记录损坏；未修改任何窗口")),
    };
    if !native::same(&t.window) || !native::marker_matches(t.window.hwnd, &t.marker) {
        let _ = std::fs::remove_file(p);
        return None;
    }
    let _gate = match Gate::acquire(lock(&t.window)) {
        Ok(g) => g,
        Err(e) => return Some(Report::fail(e)),
    };
    let r = restore(&t);
    if r.restored {
        let _ = std::fs::remove_file(p);
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_requires_completed_recovery_but_not_identical_unrelated_styles() {
        for (code, restored, exact, expected) in [
            ("reset", true, true, true),
            (STYLE_CHANGED, true, false, true),
            (STYLE_CHANGED, false, false, false),
            ("cancelled_restored", true, true, false),
            ("cancelled_restored", true, false, false),
            ("input_interrupted_restored", true, false, false),
            ("恢复核验失败", false, false, false),
            ("辅助进程中断，兜底恢复已完成", true, true, false),
        ] {
            let report = Report {
                code: code.into(),
                restored,
                exact,
                elapsed_ms: 3000,
            };
            assert_eq!(report.can_confirm(), expected, "{report:?}");
        }
    }
    #[test]
    fn worker_report_round_trip() {
        let r = Event::Done {
            report: Report {
                code: "reset".into(),
                restored: true,
                exact: true,
                elapsed_ms: 3129,
            },
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed,
            Event::Done {
                report: Report {
                    exact: true,
                    elapsed_ms: 3129,
                    ..
                }
            }
        ));
    }
}
