#![windows_subsystem = "windows"]
mod client;
use slint::ComponentHandle;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
slint::include_modules!();

fn apply(ui: &RepairWindow, state: client::State) {
    let (status, hint) = state
        .message
        .split_once(" · ")
        .unwrap_or((&state.message, &state.automatic_status));
    ui.set_status(status.into());
    ui.set_hint(hint.into());
    ui.set_automatic(state.automatic);
    ui.set_autostart(state.autostart);
    ui.set_tray_only(state.tray_only);
    ui.set_busy(state.busy);
    ui.set_can_repair(state.can_repair);
    ui.set_confirming(state.awaiting_confirmation);
    ui.set_connected(true);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new()?))?;
    let ui = RepairWindow::new()?;
    let args: Vec<_> = std::env::args().collect();
    let preview = args.iter().any(|s| s == "--preview");
    if !preview {
        let theme_path = std::env::current_exe()?
            .parent()
            .and_then(|p| p.parent())
            .ok_or("invalid UI directory")?
            .join("data")
            .join("ui-theme.txt");
        let saved = std::fs::read_to_string(&theme_path).unwrap_or_default();
        ui.set_theme(match saved.trim() {
            "light" => 1,
            "dark" => 2,
            _ => 1,
        });
        let weak = ui.as_weak();
        ui.on_theme_changed(move |theme| {
            let value = match theme {
                1 => "light",
                2 => "dark",
                _ => "light",
            };
            let result = std::fs::create_dir_all(theme_path.parent().unwrap())
                .and_then(|_| std::fs::write(&theme_path, value));
            if result.is_err() {
                if let Some(ui) = weak.upgrade() {
                    ui.set_notice("主题已切换，但无法保存；请检查程序目录是否可写".into());
                }
            }
        });
    }
    if let Some(i) = args.iter().position(|s| s == "--size") {
        if let Some((w, h)) = args.get(i + 1).and_then(|v| v.split_once('x')) {
            ui.window()
                .set_size(slint::LogicalSize::new(w.parse()?, h.parse()?));
        }
    }
    ui.on_tray(|| {
        let _ = slint::quit_event_loop();
    });
    if preview {
        if args.iter().any(|s| s == "--dark") {
            ui.invoke_preview_dark();
        }
        let weak = ui.as_weak();
        let menu = args.iter().any(|s| s == "--menu");
        let license = args.iter().any(|s| s == "--license");
        let toggle_theme = args.iter().any(|s| s == "--toggle-theme");
        slint::Timer::single_shot(Duration::from_millis(250), move || {
            if let Some(ui) = weak.upgrade() {
                if toggle_theme {
                    ui.invoke_toggle_theme();
                }
                if license {
                    ui.invoke_preview_license();
                } else if menu {
                    ui.invoke_preview_menu();
                }
            }
        });
        ui.set_status("界面预览".into());
        ui.set_hint("".into());
        ui.set_connected(true);
        ui.set_can_repair(true);
        ui.set_automatic(true);
        ui.set_confirming(args.iter().any(|s| s == "--confirm"));
        ui.on_command(|command, _| {
            if command == "exit" {
                let _ = slint::quit_event_loop();
            }
        });
    } else {
        let (tx, rx) = mpsc::sync_channel::<(String, bool, u64)>(4);
        let epoch = Arc::new(AtomicU64::new(0));
        let weak = ui.as_weak();
        let generation = epoch.clone();
        ui.on_command(move |command, enabled| {
            if let Some(ui) = weak.upgrade() {
                if ui.get_sending() {
                    return;
                }
                let id = generation.fetch_add(1, Ordering::AcqRel) + 1;
                ui.set_sending(true);
                if tx.try_send((command.to_string(), enabled, id)).is_err() {
                    ui.set_sending(false);
                    ui.set_notice("后台正忙，请稍后重试".into());
                }
            }
        });
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let mut first = true;
            loop {
                let request = if first {
                    first = false;
                    None
                } else {
                    match rx.recv_timeout(Duration::from_millis(500)) {
                        Ok(v) => Some(v),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => break,
                    }
                };
                let (command, enabled, id) = request
                    .unwrap_or_else(|| ("status".into(), false, epoch.load(Ordering::Acquire)));
                let result = client::send(&command, enabled);
                let current_epoch = epoch.clone();
                if weak
                    .upgrade_in_event_loop(move |ui| {
                        if current_epoch.load(Ordering::Acquire) != id {
                            return;
                        }
                        if command != "status" {
                            ui.set_sending(false)
                        }
                        match result {
                            Ok(state) => {
                                apply(&ui, state);
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
                            Err(error) => {
                                ui.set_connected(false);
                                ui.set_status("无法连接后台".into());
                                ui.set_hint(error.into());
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }
    if let Some(i) = args.iter().position(|s| s == "--capture") {
        let path = args.get(i + 1).ok_or("missing capture path")?.clone();
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(750), move || {
            if let Some(ui) = weak.upgrade() {
                let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                    let pixels = ui.window().take_snapshot()?;
                    let mut encoder = png::Encoder::new(
                        std::fs::File::create(&path)?,
                        pixels.width(),
                        pixels.height(),
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()?
                        .write_image_data(pixels.as_bytes())?;
                    Ok(())
                })();
                if let Err(e) = result {
                    let _ = std::fs::write(format!("{path}.error.txt"), e.to_string());
                }
            }
            let _ = slint::quit_event_loop();
        });
    }
    ui.run()?;
    Ok(())
}
