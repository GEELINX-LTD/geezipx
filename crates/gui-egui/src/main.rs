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
                            | "--prepare-shell-upgrade"
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
    let (maintenance, menu) = match arguments.as_slice() {
        [argument] => (Some(argument), None),
        [argument, choice] if argument == "--repair-shell-integration" => (
            Some(argument),
            Some(match choice.to_str() {
                Some("--menu=1") => true,
                Some("--menu=0") => false,
                _ => return Err("Expected --menu=0 or --menu=1".into()),
            }),
        ),
        _ => (None, None),
    };
    if let Some(argument) = maintenance {
        if argument == "--prepare-shell-upgrade" {
            integration::prepare_windows_upgrade().map_err(std::io::Error::other)?;
            return Ok(());
        }
        let operation = match argument.to_str() {
            Some("--shell-integration-status") => Some(Operation::Inspect),
            Some("--remove-shell-integration") => Some(Operation::Remove),
            Some("--register-shell-integration") => Some(Operation::Apply {
                selected: FORMATS.iter().map(|f| f.available()).collect(),
                menu: true,
                language: Default::default(),
            }),
            Some("--repair-shell-integration") => Some(Operation::Repair { menu }),
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
            println!("GeeZipX {}\n\n用法：geezipx-egui [--] [归档 | 文件或文件夹…]\n单个归档会打开浏览，其他输入会加入新建压缩包。\n\n系统快捷操作：\n  --shell-action <compress|compress-zip|extract|extract-here|extract-folder> -- <路径…>\n\n系统集成维护（当前用户，无需启动窗口）：\n  --shell-integration-status\n  --register-shell-integration\n  --repair-shell-integration [--menu=0|--menu=1]\n  --remove-shell-integration", env!("CARGO_PKG_VERSION"));
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
    let options = native_options(action);
    eframe::run_native(
        "GeeZipX",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, action, paths)))),
    )?;
    Ok(())
}

fn native_options(action: Action) -> eframe::NativeOptions {
    let extraction = action.is_extraction();
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("GeeZipX")
        .with_app_id(integration::APP_ID)
        .with_icon(theme::icon())
        .with_decorations(false)
        .with_transparent(true);
    eframe::NativeOptions {
        viewport: if extraction {
            viewport
                .with_inner_size(app::SHELL_WINDOW_SIZE)
                .with_min_inner_size(app::SHELL_WINDOW_MIN_SIZE)
        } else {
            viewport
                .with_inner_size([1000.0, 680.0])
                .with_min_inner_size(window_frame::MIN_INNER_SIZE)
        },
        // Shell jobs must neither restore nor overwrite the manager's geometry.
        persist_window: !extraction,
        // eframe 0.33 still loads saved geometry when persist_window is false.
        // This hook runs after restoration, before the native window is created.
        window_builder: if extraction {
            Some(Box::new(|viewport| {
                viewport
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_min_inner_size(app::SHELL_WINDOW_MIN_SIZE)
                    .with_inner_size(app::SHELL_WINDOW_SIZE)
                    .with_maximized(false)
                    .with_fullscreen(false)
            }))
        } else {
            None
        },
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn extraction_shortcuts_start_small_without_restoring_the_manager_window() {
        for action in [Action::Extract, Action::ExtractHere, Action::ExtractFolder] {
            let mut options = native_options(action);
            assert_eq!(
                options.viewport.inner_size,
                Some(app::SHELL_WINDOW_SIZE.into())
            );
            assert_eq!(options.viewport.decorations, Some(false));
            assert_eq!(options.viewport.transparent, Some(true));
            assert_eq!(
                options.viewport.min_inner_size,
                Some(app::SHELL_WINDOW_MIN_SIZE.into())
            );
            assert!(!options.persist_window);
            let restored = eframe::egui::ViewportBuilder::default()
                .with_decorations(true)
                .with_transparent(false)
                .with_min_inner_size(window_frame::MIN_INNER_SIZE)
                .with_inner_size([1400.0, 900.0])
                .with_maximized(true)
                .with_fullscreen(true);
            let viewport = options.window_builder.take().unwrap()(restored);
            assert_eq!(viewport.inner_size, Some(app::SHELL_WINDOW_SIZE.into()));
            assert_eq!(viewport.maximized, Some(false));
            assert_eq!(viewport.fullscreen, Some(false));
            assert_eq!(viewport.decorations, Some(false));
            assert_eq!(viewport.transparent, Some(true));
            assert_eq!(
                viewport.min_inner_size,
                Some(app::SHELL_WINDOW_MIN_SIZE.into())
            );
        }
        for action in [Action::Open, Action::Compress, Action::CompressZip] {
            let options = native_options(action);
            assert_eq!(options.viewport.inner_size, Some([1000.0, 680.0].into()));
            assert_eq!(options.viewport.decorations, Some(false));
            assert!(options.persist_window);
        }
    }
}
