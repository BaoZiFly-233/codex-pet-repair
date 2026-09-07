#![windows_subsystem = "windows"]
mod client;
#[cfg(feature = "dev-tools")]
mod preview;
#[path = "../../../shared/protocol.rs"]
mod protocol;
#[path = "../../../shared/signal.rs"]
mod signal;
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
slint::include_modules!();

fn configure_content_sizing(ui: &RepairWindow) {
    let initial = slint::LogicalSize::new(432.0, 432.0);
    ui.window()
        .set_size(initial.to_physical(ui.window().scale_factor()));
    let fitted = Rc::new(std::cell::Cell::new(Some(initial)));
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    ui.on_content_resized(move || {
        let weak = weak.clone();
        let fitted = fitted.clone();
        // Measure after Slint has applied wrapping and conditional rows, not inside an IPC update.
        timer.start(slint::TimerMode::SingleShot, Duration::ZERO, move || {
            let (Some(ui), Some(previous)) = (weak.upgrade(), fitted.get()) else {
                return;
            };
            let current = ui.window().size().to_logical(ui.window().scale_factor());
            if (current.width - previous.width).abs() > 1.0
                || (current.height - previous.height).abs() > 1.0
            {
                fitted.set(None); // A user-sized window keeps its size and can scroll when needed.
                return;
            }
            let next = slint::LogicalSize::new(current.width, ui.get_content_height().ceil() + 2.0);
            if (current.height - next.height).abs() > 0.5 {
                fitted.set(Some(next));
                ui.window()
                    .set_size(next.to_physical(ui.window().scale_factor()));
            }
        });
    });
    ui.invoke_content_resized();
}

fn apply(ui: &RepairWindow, state: &protocol::UiState) {
    macro_rules! update {
        ($get:ident, $set:ident, $value:expr) => {
            if ui.$get() != $value {
                ui.$set(($value).into());
            }
        };
    }
    update!(get_status, set_status, state.status.as_str());
    update!(get_hint, set_hint, state.hint.as_str());
    update!(
        get_automatic_status,
        set_automatic_status,
        state.automatic_status.as_str()
    );
    update!(get_automatic, set_automatic, state.automatic);
    update!(get_autostart, set_autostart, state.autostart);
    update!(get_tray_only, set_tray_only, state.tray_only);
    update!(get_busy, set_busy, state.busy);
    update!(get_can_repair, set_can_repair, state.can_repair);
    update!(get_confirming, set_confirming, state.awaiting_confirmation);
}
fn restore_settings(ui: &RepairWindow, state: &protocol::UiState) {
    ui.set_automatic(state.automatic);
    ui.set_autostart(state.autostart);
    ui.set_tray_only(state.tray_only);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    #[cfg(not(feature = "dev-tools"))]
    if args.iter().any(|s| {
        matches!(
            s.as_str(),
            "--preview"
                | "--capture"
                | "--size"
                | "--dark"
                | "--menu"
                | "--license"
                | "--confirm"
                | "--disabled"
                | "--busy"
                | "--startup"
                | "--toggle-theme"
        )
    }) {
        return Err("预览和截图需要 dev-tools 构建".into());
    }
    slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new()?))?;
    let ui = RepairWindow::new()?;
    if !args.iter().any(|s| s == "--size") {
        configure_content_sizing(&ui);
    }
    ui.on_tray(|| {
        let _ = slint::quit_event_loop();
    });
    #[cfg(feature = "dev-tools")]
    if args.iter().any(|s| s == "--preview") {
        preview::configure(&ui, &args)?;
        ui.run()?;
        return Ok(());
    }
    let theme_path = std::env::current_exe()?
        .parent()
        .and_then(|p| p.parent())
        .ok_or("invalid UI directory")?
        .join("data")
        .join("ui-theme.txt");
    ui.set_theme(
        if std::fs::read_to_string(&theme_path)
            .unwrap_or_default()
            .trim()
            == "dark"
        {
            2
        } else {
            1
        },
    );
    let weak = ui.as_weak();
    ui.on_theme_changed(move |theme| {
        let value = if theme == 2 { "dark" } else { "light" };
        if std::fs::create_dir_all(theme_path.parent().unwrap())
            .and_then(|_| std::fs::write(&theme_path, value))
            .is_err()
        {
            if let Some(ui) = weak.upgrade() {
                ui.set_notice("主题已切换，但无法保存；请检查程序目录是否可写".into());
            }
        }
    });
    let countdown = Rc::new(slint::Timer::default());
    let last = Rc::new(RefCell::new(protocol::UiState::default()));
    let (tx, rx) = mpsc::sync_channel::<(String, bool, u64)>(4);
    let epoch = Arc::new(AtomicU64::new(0));
    let wake = Arc::new(signal::Signal::new(None)?);
    let changed = signal::Signal::new(Some(&protocol::change_event(client::session())))?;
    let weak = ui.as_weak();
    let generation = epoch.clone();
    let command_wake = wake.clone();
    let confirmed = last.clone();
    ui.on_command(move |command, enabled| {
        if let Some(ui) = weak.upgrade() {
            if ui.get_sending() {
                return;
            }
            let id = generation.fetch_add(1, Ordering::AcqRel) + 1;
            ui.set_sending(true);
            ui.set_notice("".into());
            if tx.try_send((command.to_string(), enabled, id)).is_err() {
                restore_settings(&ui, &confirmed.borrow());
                ui.set_sending(false);
                ui.set_notice("后台正忙，请稍后重试".into());
            } else {
                command_wake.set();
            }
        }
    });
    let weak = ui.as_weak();
    let current_epoch = epoch.clone();
    let (results_tx, results_rx) = mpsc::channel();
    let delivery_wake = ui.as_weak();
    std::thread::spawn(move || {
        let mut first = true;
        loop {
            if !first && signal::Signal::wait(&[&wake, &changed], 10000).is_err() {
                break;
            }
            first = false;
            let (command, enabled, id) = match rx.try_recv() {
                Ok(request) => request,
                Err(mpsc::TryRecvError::Empty) => {
                    ("ui_status".into(), false, epoch.load(Ordering::Acquire))
                }
                Err(_) => break,
            };
            let result = client::send(&command, enabled);
            if results_tx.send((command, id, result)).is_err() {
                break;
            }
            if delivery_wake
                .upgrade_in_event_loop(|ui| ui.invoke_receive())
                .is_err()
            {
                break;
            }
        }
    });
    ui.on_receive(move || {
        while let Ok((command, id, result)) = results_rx.try_recv() {
            if id != current_epoch.load(Ordering::Acquire) {
                continue;
            }
            if let Some(ui) = weak.upgrade() {
                if command != "ui_status" {
                    ui.set_sending(false);
                }
                match result {
                    Ok(state) => {
                        apply(&ui, &state);
                        ui.set_connected(true);
                        ui.set_wait_seconds(state.wait_remaining_ms.div_ceil(1000) as i32);
                        countdown.stop();
                        if state.wait_remaining_ms > 0 {
                            let until =
                                Instant::now() + Duration::from_millis(state.wait_remaining_ms);
                            let weak = ui.as_weak();
                            let timer = Rc::downgrade(&countdown);
                            countdown.start(
                                slint::TimerMode::Repeated,
                                Duration::from_secs(1),
                                move || {
                                    let remaining = until
                                        .saturating_duration_since(Instant::now())
                                        .as_millis()
                                        .div_ceil(1000)
                                        as i32;
                                    if let Some(ui) = weak.upgrade() {
                                        ui.set_wait_seconds(remaining);
                                    }
                                    if remaining == 0 {
                                        if let Some(timer) = timer.upgrade() {
                                            timer.stop();
                                        }
                                    }
                                },
                            );
                        }
                        *last.borrow_mut() = state;
                        match command.as_str() {
                            "exit" => {
                                let _ = slint::quit_event_loop();
                            }
                            "open_log" => ui.set_notice("已打开日志文件".into()),
                            "open_log_folder" => ui.set_notice("已打开日志文件夹".into()),
                            "copy_diagnostics" => {
                                ui.set_notice("已复制诊断摘要，可粘贴到反馈中".into())
                            }
                            _ => {}
                        }
                    }
                    Err(client::Error::Action(error, state)) => {
                        if let Some(state) = state {
                            apply(&ui, &state);
                            *last.borrow_mut() = state;
                        }
                        restore_settings(&ui, &last.borrow());
                        ui.set_notice(error.into());
                    }
                    Err(client::Error::Connection(error)) => {
                        restore_settings(&ui, &last.borrow());
                        ui.set_connected(false);
                        ui.set_status("无法连接后台".into());
                        ui.set_hint(error.into());
                        countdown.stop();
                        ui.set_wait_seconds(0);
                    }
                }
            }
        }
    });
    ui.run()?;
    Ok(())
}
