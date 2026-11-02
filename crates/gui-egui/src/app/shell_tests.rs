use super::tests::{context, finish_job};
use super::*;

#[test]
fn shell_compression_keeps_the_complete_selection_and_never_replaces_output() {
    let context = context();
    let root = tempfile::tempdir().unwrap();
    let sources = [
        root.path().join("中文 空格.txt"),
        root.path().join("--help.txt"),
    ];
    for (index, source) in sources.iter().enumerate() {
        std::fs::write(source, format!("content {index}")).unwrap();
    }
    let mut app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::Compress, sources.to_vec());
    assert!(app.create_open && app.active.is_none());
    assert_eq!(app.draft.sources, sources);
    app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::CompressZip, sources.to_vec());
    finish_job(&mut app, &context);
    let output = root.path().join("中文 空格.zip");
    let original = std::fs::read(&output).unwrap();
    let archive = engine::list(output.clone(), None, &Default::default()).unwrap();
    for source in &sources {
        assert!(archive
            .entries
            .iter()
            .any(|entry| entry.path == source.file_name().unwrap().to_string_lossy()));
    }
    app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::CompressZip, sources.to_vec());
    finish_job(&mut app, &context);
    assert!(app.create_open && app.form_error.is_some());
    assert_eq!(std::fs::read(output).unwrap(), original);
}

#[test]
fn shell_extraction_waits_for_listing_and_uses_the_requested_destination() {
    for action in [Action::Extract, Action::ExtractHere, Action::ExtractFolder] {
        let context = context();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("中文.txt");
        std::fs::write(&source, "archive content").unwrap();
        let mut creator = App::empty(Preferences::default());
        creator.accept_launch(&context, Action::CompressZip, vec![source.clone()]);
        finish_job(&mut creator, &context);
        let archive = root.path().join("中文.zip");
        std::fs::write(&source, "keep existing").unwrap();
        let mut app = App::empty(Preferences::default());
        app.accept_launch(&context, action, vec![archive.clone()]);
        assert!(app.active.is_some());
        assert!(app.shell_task.is_some());
        finish_job(&mut app, &context);
        if action == Action::Extract {
            use super::tests::{assert_action_visible, draw};
            for size in [[560.0, 440.0], [480.0, 440.0]] {
                draw(&mut app, &context, size);
                let output = draw(&mut app, &context, size);
                assert_action_visible(&output, size, "开始解压");
                assert_action_visible(&output, size, "取消");
            }
            let options = app.extraction.as_mut().unwrap();
            assert_eq!(options.destination.path, root.path().join("中文"));
            assert!(!options.destination.path.exists());
            let destination = root.path().join("chosen output");
            options.destination.set(destination.clone());
            std::fs::write(&destination, "keep conflicting file").unwrap();
            app.begin_extraction(&context);
            finish_job(&mut app, &context);
            assert!(app.form_error.is_some() && app.extraction.is_some());
            assert!(!app.close_when_finished);
            assert_eq!(
                std::fs::read_to_string(&destination).unwrap(),
                "keep conflicting file"
            );
            std::fs::remove_file(&destination).unwrap();
            for language in Language::ALL {
                app.preferences.language = language;
                for dark in [false, true] {
                    app.preferences.dark = dark;
                    theme::apply(&context, dark);
                    for size in [[560.0, 440.0], [480.0, 440.0]] {
                        draw(&mut app, &context, size);
                        let output = draw(&mut app, &context, size);
                        assert_action_visible(&output, size, tr!(language, StartExtraction));
                        assert_action_visible(&output, size, tr!(language, Cancel));
                    }
                }
            }
            app.begin_extraction(&context);
            finish_job(&mut app, &context);
            assert_eq!(
                std::fs::read_to_string(root.path().join("chosen output/中文.txt")).unwrap(),
                "archive content"
            );
        } else if action == Action::ExtractFolder {
            assert_eq!(
                std::fs::read_to_string(root.path().join("中文/中文.txt")).unwrap(),
                "archive content"
            );
        }
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "keep existing");
        assert!(app.shell_extraction.is_none());
        assert!(!app.history[0].failed);
        assert!(app.close_when_finished);
        assert!(app.preferences.recent.is_empty());
    }
}

#[test]
fn failed_shell_open_never_extracts_a_different_archive() {
    let context = context();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.txt");
    std::fs::write(&source, "content").unwrap();
    let mut creator = App::empty(Preferences::default());
    creator.accept_launch(&context, Action::CompressZip, vec![source]);
    finish_job(&mut creator, &context);
    let missing = root.path().join("missing.zip");
    let mut app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::ExtractHere, vec![missing.clone()]);
    finish_job(&mut app, &context);
    assert_eq!(app.retry_path, Some(missing));
    assert!(app.shell_extraction.is_some());
    assert!(!app.close_when_finished);
    app.open_archive(&context, root.path().join("source.zip"), None);
    finish_job(&mut app, &context);
    assert!(app.extraction.is_none() && app.history.is_empty());
    assert!(app.shell_extraction.is_none());
}

#[test]
fn shell_progress_stays_compact_and_waits_for_worker_cleanup_when_closed() {
    use super::tests::{assert_action_visible, draw};
    for language in Language::ALL {
        for dark in [false, true] {
            let context = context();
            theme::apply(&context, dark);
            let mut app = App::empty(Preferences {
                language,
                dark,
                ..Default::default()
            });
            app.shell_task = Some(shell_progress::ShellTask::new("中文 大归档.zip".into()));
            let (release, receiver) = std::sync::mpsc::sync_channel(1);
            app.start(&context, Kind::Extract, move |control| {
                receiver.recv().unwrap();
                control.check()?;
                unreachable!("the test cancels before releasing the worker")
            });
            app.active.as_ref().unwrap().job.control.update(|progress| {
                progress.stage = tr!(language, Extracting).into();
                progress.entry = "目录/当前文件.txt".into();
                progress.bytes = 500;
                progress.total = 1000;
            });
            for size in [SHELL_WINDOW_SIZE, [480.0, 300.0]] {
                draw(&mut app, &context, size);
                let output = draw(&mut app, &context, size);
                assert_action_visible(&output, size, tr!(language, CancelTask));
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape {
                            Some(text.galley.text())
                        } else {
                            None
                        }
                    })
                    .collect();
                assert!(texts.contains(&tr!(language, Extracting)));
                assert!(texts.contains(&"50%"));
                assert_action_visible(&output, size, "目录/当前文件.txt");
                if size == SHELL_WINDOW_SIZE {
                    let speed = texts.iter().find(|text| text.contains("/s")).unwrap();
                    assert_action_visible(&output, size, speed);
                }
                assert!(!texts.contains(&tr!(language, FileMenu)));
                assert!(!texts.contains(&tr!(language, New)));
                assert!(!texts.contains(&tr!(language, Tasks)));
            }
            draw(&mut app, &context, SHELL_WINDOW_SIZE);
            let position = egui::pos2(SHELL_WINDOW_SIZE[0] - 23.0, 17.0);
            shell_pointer(&mut app, &context, position, Some(true));
            let output = shell_pointer(&mut app, &context, position, Some(false));
            assert!(app.active.as_ref().unwrap().job.control.is_cancelled());
            assert!(app.close_when_finished);
            assert!(!output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close)));
            release.send(()).unwrap();
            finish_job(&mut app, &context);
            let output = draw(&mut app, &context, SHELL_WINDOW_SIZE);
            assert!(output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close)));
            assert!(app.history[0].cancelled && !app.history[0].failed);
        }
    }
}

#[test]
fn shell_extraction_retries_encrypted_content_in_the_same_small_window() {
    use super::tests::{assert_action_visible, draw};
    let context = context();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("加密.txt");
    std::fs::write(&source, "secret content").unwrap();
    let mut creator = App::empty(Preferences::default());
    creator.add_sources(vec![source]);
    creator.draft.show_password = true;
    creator.draft.password = "secret".into();
    creator.draft.confirm = "secret".into();
    creator.create_archive(&context);
    finish_job(&mut creator, &context);
    assert!(!creator.history[0].failed);

    let mut app = App::empty(Preferences::default());
    app.accept_launch(
        &context,
        Action::ExtractFolder,
        vec![root.path().join("加密.zip")],
    );
    finish_job(&mut app, &context);
    assert!(app.retry_path.is_some() && app.archive.is_none());
    assert!(!app.close_when_finished);
    let destination = root.path().join("加密");
    for password in ["wrong", "secret"] {
        app.read_password = password.into();
        draw(&mut app, &context, SHELL_WINDOW_SIZE);
        let output = draw(&mut app, &context, SHELL_WINDOW_SIZE);
        assert_action_visible(&output, SHELL_WINDOW_SIZE, "重新打开");
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    SHELL_WINDOW_SIZE.into(),
                )),
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |context| app.render(context),
        );
        assert!(app.read_password.is_empty());
        finish_job(&mut app, &context);
        if password == "wrong" {
            assert!(!app.close_when_finished);
            assert!(app.retry_path.is_some());
            assert!(!destination.join("加密.txt").exists());
        }
    }
    assert!(app.close_when_finished && app.extraction.is_none());
    assert_eq!(
        std::fs::read_to_string(destination.join("加密.txt")).unwrap(),
        "secret content"
    );
}

#[test]
fn shell_listing_failure_can_retry_the_original_request_and_never_saves_preferences() {
    let context = context();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("missing.zip");
    let mut app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::ExtractFolder, vec![path.clone()]);
    finish_job(&mut app, &context);
    assert!(app.retry_path.is_some() && !app.close_when_finished);
    for _ in 0..3 {
        let output = super::tests::draw(&mut app, &context, SHELL_WINDOW_SIZE);
        super::tests::assert_action_visible(&output, SHELL_WINDOW_SIZE, "重新打开");
    }
    let source = root.path().join("missing.txt");
    std::fs::write(&source, "retried content").unwrap();
    let mut creator = App::empty(Preferences::default());
    creator.accept_launch(&context, Action::CompressZip, vec![source]);
    finish_job(&mut creator, &context);
    app.open_archive(&context, path, None);
    finish_job(&mut app, &context);
    assert!(app.close_when_finished);
    assert_eq!(
        std::fs::read_to_string(root.path().join("missing/missing.txt")).unwrap(),
        "retried content"
    );
    assert!(app.preferences.recent.is_empty());
    assert!(!eframe::App::persist_egui_memory(&app));
    struct UnwritableStorage;
    impl eframe::Storage for UnwritableStorage {
        fn get_string(&self, _: &str) -> Option<String> {
            None
        }
        fn set_string(&mut self, _: &str, _: String) {
            panic!("shell jobs must not overwrite manager preferences")
        }
        fn flush(&mut self) {}
    }
    eframe::App::save(&mut app, &mut UnwritableStorage);
}

fn shell_pointer(
    app: &mut App,
    context: &egui::Context,
    position: egui::Pos2,
    pressed: Option<bool>,
) -> egui::FullOutput {
    let mut events = vec![egui::Event::PointerMoved(position)];
    if let Some(pressed) = pressed {
        events.push(egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                SHELL_WINDOW_SIZE.into(),
            )),
            time: Some(context.input(|input| input.time) + 0.05),
            events,
            ..Default::default()
        },
        |context| app.render(context),
    )
}

#[test]
fn shell_frame_keeps_round_corners_and_controls_above_destination_dialogs() {
    use super::tests::{archive, assert_action_visible, mesh_alpha_at};
    for dark in [false, true] {
        for options in [false, true] {
            let sizes = if options {
                [[560.0, 440.0], [480.0, 440.0]]
            } else {
                [SHELL_WINDOW_SIZE, SHELL_WINDOW_MIN_SIZE]
            };
            for size in sizes {
                for expanded in [false, true] {
                    let context = context();
                    theme::apply(&context, dark);
                    let mut app = App::empty(Preferences {
                        dark,
                        ..Default::default()
                    });
                    app.shell_task = Some(shell_progress::ShellTask::new("中文 大归档.zip".into()));
                    app.archive = Some(archive());
                    if options {
                        app.show_extract(true);
                    }
                    let mut output = None;
                    for _ in 0..3 {
                        let mut input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                size.into(),
                            )),
                            ..Default::default()
                        };
                        input
                            .viewports
                            .get_mut(&egui::ViewportId::ROOT)
                            .unwrap()
                            .maximized = Some(expanded);
                        output = Some(context.run(input, |context| app.render(context)));
                    }
                    let output = output.unwrap();
                    assert_action_visible(&output, size, tr!(Language::default(), ExtractFiles));
                    if options {
                        assert_action_visible(
                            &output,
                            size,
                            tr!(Language::default(), StartExtraction),
                        );
                    }
                    let meshes = context.tessellate(output.shapes, output.pixels_per_point);
                    for x in [0.5, size[0] - 0.5] {
                        for y in [0.5, size[1] - 0.5] {
                            let alpha = mesh_alpha_at(&meshes, egui::pos2(x, y));
                            assert!(if expanded { alpha > 0.99 } else { alpha < 0.01 }, "shell corner {x},{y}, alpha={alpha}, dark={dark}, options={options}, expanded={expanded}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shell_title_controls_minimize_drag_and_close_with_destination_dialog_open() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.shell_task = Some(shell_progress::ShellTask::new("archive.zip".into()));
    app.archive = Some(super::tests::archive());
    app.show_extract(true);
    for _ in 0..3 {
        super::tests::draw(&mut app, &context, SHELL_WINDOW_SIZE);
    }
    let minimize = egui::pos2(SHELL_WINDOW_SIZE[0] - 91.0, 17.0);
    shell_pointer(&mut app, &context, minimize, Some(true));
    let output = shell_pointer(&mut app, &context, minimize, Some(false));
    assert!(output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Minimized(true))));
    let title = egui::pos2(300.0, 17.0);
    shell_pointer(&mut app, &context, title, Some(true));
    let output = shell_pointer(&mut app, &context, title + egui::vec2(15.0, 0.0), None);
    assert!(output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::StartDrag)));
    shell_pointer(&mut app, &context, title, Some(false));
    let close = egui::pos2(SHELL_WINDOW_SIZE[0] - 23.0, 17.0);
    shell_pointer(&mut app, &context, close, Some(true));
    let output = shell_pointer(&mut app, &context, close, Some(false));
    assert!(output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Close)));
    assert!(!app.close_prompt);
}
