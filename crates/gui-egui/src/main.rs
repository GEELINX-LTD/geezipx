#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod fonts;
mod theme;
mod widgets;
mod window_frame;
use geezipx_egui::{
    integration::{self, Operation, FORMATS},
    launch::{self, Action, Request},
};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("GeeZipX: {error}");
            // Interactive shell failures need to remain visible in a GUI-subsystem binary.
            if !std::env::args_os().any(|arg| {
                matches!(
                    arg.to_str(),
                    Some(
                        "--remove-shell-integration"
                            | "--repair-shell-integration"
                            | "--shell-integration-status"
                            | "--register-shell-integration"
                            | "-Embedding"
                    )
                )
            }) {
                rfd::MessageDialog::new()
                    .set_title("GeeZipX")
                    .set_description(error.to_string())
                    .set_level(rfd::MessageLevel::Error)
                    .show();
            }
            std::process::ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if let [argument] = arguments.as_slice() {
        let operation = match argument.to_str() {
            Some("--shell-integration-status") => Some(Operation::Inspect),
            Some("--remove-shell-integration") => Some(Operation::Remove),
            Some("--register-shell-integration") => Some(Operation::Apply {
                selected: FORMATS.iter().map(|f| f.available()).collect(),
                menu: true,
                language: Default::default(),
            }),
            Some("--repair-shell-integration") => {
                let snapshot =
                    integration::execute(Operation::Inspect).map_err(std::io::Error::other)?;
                if !snapshot.menu && !snapshot.associations.iter().any(|a| a.registered) {
                    return Ok(());
                }
                Some(Operation::Apply {
                    selected: snapshot
                        .associations
                        .iter()
                        .map(|a| {
                            if cfg!(target_os = "windows") {
                                a.registered
                            } else {
                                a.is_default
                            }
                        })
                        .collect(),
                    menu: snapshot.menu,
                    language: Default::default(),
                })
            }
            _ => None,
        };
        if let Some(operation) = operation {
            let snapshot = integration::execute(operation).map_err(std::io::Error::other)?;
            println!(
                "registered={} menu={}",
                snapshot
                    .associations
                    .iter()
                    .filter(|a| a.registered)
                    .count(),
                snapshot.menu
            );
            return Ok(());
        }
    }
    let (action, paths) = match launch::parse(arguments).map_err(std::io::Error::other)? {
        Request::Empty => (Action::Open, Vec::new()),
        Request::Paths(action, paths) => (action, paths),
        Request::Help => {
            println!("GeeZipX Native {}\n\n用法：geezipx-egui [--] [归档 | 文件或文件夹…]\n单个归档会打开浏览，其他输入会加入新建压缩包。\n\n系统快捷操作：\n  --shell-action <compress|compress-zip|extract|extract-here|extract-folder> -- <路径…>\n\n系统集成维护（当前用户，无需启动窗口）：\n  --shell-integration-status\n  --register-shell-integration\n  --repair-shell-integration\n  --remove-shell-integration", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Request::Version => {
            println!("geezipx-egui {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Request::Embedding => {
            #[cfg(windows)]
            {
                integration::com::run().map_err(std::io::Error::other)?;
                return Ok(());
            }
            #[cfg(not(windows))]
            return Err(std::io::Error::other("Explorer COM activation requires Windows").into());
        }
        Request::ActionFile(path) => {
            #[cfg(windows)]
            {
                integration::handoff::read(&path).map_err(std::io::Error::other)?
            }
            #[cfg(not(windows))]
            {
                let _ = path;
                return Err(std::io::Error::other("Explorer selections require Windows").into());
            }
        }
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("GeeZipX · Native")
            .with_decorations(false)
            .with_transparent(true)
            .with_app_id(integration::APP_ID)
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size(window_frame::MIN_INNER_SIZE)
            .with_icon(theme::icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "GeeZipX Native",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, action, paths)))),
    )?;
    Ok(())
}
