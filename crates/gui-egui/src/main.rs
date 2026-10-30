#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod fonts;
mod theme;
mod widgets;

fn main() -> eframe::Result {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("GeeZipX Native {}\n\n用法：geezipx-egui [归档 | 文件或文件夹…]\n单个归档会打开浏览，其他输入会加入新建压缩包。", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if arguments.iter().any(|arg| arg == "--version") {
        println!("geezipx-egui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("GeeZipX · Native")
            .with_app_id("com.geelinx.geezipx.native")
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(theme::icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let paths = arguments
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect();
    eframe::run_native(
        "GeeZipX Native",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, paths)))),
    )
}
