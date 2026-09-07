use crate::RepairWindow;
use slint::ComponentHandle;
use std::time::Duration;

pub fn configure(ui: &RepairWindow, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(size) = args
        .iter()
        .position(|s| s == "--size")
        .and_then(|i| args.get(i + 1))
    {
        let (w, h) = size.split_once('x').ok_or("size must be WIDTHxHEIGHT")?;
        ui.window().set_size(
            slint::LogicalSize::new(w.parse()?, h.parse()?).to_physical(ui.window().scale_factor()),
        );
    }
    ui.set_theme(if args.iter().any(|s| s == "--dark") {
        2
    } else {
        1
    });
    let busy = args.iter().any(|s| s == "--busy");
    ui.set_status(if busy { "正在修复" } else { "可以修复" }.into());
    ui.set_hint(
        if busy {
            "约需 3 秒，请暂时不要拖动宠物"
        } else {
            "点击“立即修复”"
        }
        .into(),
    );
    ui.set_automatic_status("自动修复已开启".into());
    ui.set_connected(!args.iter().any(|s| s == "--disabled"));
    ui.set_can_repair(true);
    ui.set_automatic(true);
    ui.set_confirming(args.iter().any(|s| s == "--confirm"));
    ui.set_busy(args.iter().any(|s| s == "--busy"));
    ui.on_command(|command, _| {
        if command == "exit" {
            let _ = slint::quit_event_loop();
        }
    });
    let weak = ui.as_weak();
    let menu = args.iter().any(|s| s == "--menu");
    let license = args.iter().any(|s| s == "--license");
    let toggle = args.iter().any(|s| s == "--toggle-theme");
    let startup = args.iter().any(|s| s == "--startup");
    if startup {
        ui.set_hint("".into());
        ui.set_automatic_status("".into());
    }
    slint::Timer::single_shot(Duration::from_millis(250), move || {
        if let Some(ui) = weak.upgrade() {
            if startup {
                ui.set_hint("约需 3 秒，请暂时不要拖动宠物".into());
                ui.set_automatic_status("自动修复已开启 · 等待你确认效果".into());
                ui.set_busy(true);
                ui.set_notice("设置未保存，请检查程序目录是否可写".into());
            }
            if toggle {
                ui.invoke_toggle_theme();
            }
            if license {
                ui.invoke_preview_license();
            } else if menu {
                ui.invoke_preview_menu();
            }
        }
    });
    if let Some(path) = args
        .iter()
        .position(|s| s == "--capture")
        .and_then(|i| args.get(i + 1))
        .cloned()
    {
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(750), move || {
            if let Some(ui) = weak.upgrade() {
                let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                    let pixels = ui.window().take_snapshot()?;
                    let size = ui.window().size().to_logical(ui.window().scale_factor());
                    std::fs::write(
                        format!("{path}.json"),
                        serde_json::to_vec(&serde_json::json!({
                            "width": size.width, "height": size.height, "content_height": ui.get_content_height(), "scroll_overflow": ui.get_scroll_overflow()
                        }))?,
                    )?;
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
                if let Err(error) = result {
                    let _ = std::fs::write(format!("{path}.error.txt"), error.to_string());
                }
            }
            let _ = slint::quit_event_loop();
        });
    }
    Ok(())
}
