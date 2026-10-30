use super::*;
use geezipx_core::archive::Entry;

fn archive() -> Archive {
    let entries = ["项目/目录/文件.txt", "notes.txt", "other.txt"]
        .map(|path| Entry {
            path: path.into(),
            size: 128,
            compressed_size: 64,
            crc32: None,
            modified: None,
            is_dir: false,
        })
        .to_vec();
    Archive {
        path: "设计.zip".into(),
        format: ArchiveFormat::Zip,
        browser: geezipx_egui::browser::Browser::new(&entries),
        entries,
        bytes: 384,
    }
}
fn context() -> egui::Context {
    let context = egui::Context::default();
    theme::fonts(&context);
    theme::apply(&context, false);
    context
}
fn draw(app: &mut App, context: &egui::Context, size: [f32; 2]) -> egui::FullOutput {
    let output = context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
            ..Default::default()
        },
        |context| app.render(context),
    );
    assert!(!output.shapes.is_empty());
    output
}
fn key(app: &mut App, context: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
    let _ = context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 680.0),
            )),
            modifiers,
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |context| app.render(context),
    );
}
fn assert_action_visible(output: &egui::FullOutput, size: [f32; 2], action: &str) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size.into());
    let text = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.text() == action => {
                Some((clipped.clip_rect, text))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing primary action {action}"));
    let bounds = egui::Rect::from_min_size(text.1.pos, text.1.galley.size());
    assert!(
        screen.intersect(text.0).contains_rect(bounds),
        "{action} clipped at {size:?}: {bounds:?}"
    );
}

#[test]
fn dialogs_keep_primary_actions_visible_at_minimum_size() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    for dark in [false, true] {
        app.preferences.dark = dark;
        theme::apply(&context, dark);
        for size in [[760.0, 480.0], [1000.0, 680.0], [1600.0, 1000.0]] {
            app.add_sources(vec![PathBuf::from("设计资源/说明.md")]);
            app.draft.show_sources = true;
            app.draft.show_password = true;
            app.draft.show_advanced = true;
            for format in 0..engine::formats::WRITE_FORMATS.len() {
                app.draft.format = format;
                for _ in 0..2 {
                    draw(&mut app, &context, size);
                }
                let output = draw(&mut app, &context, size);
                assert_action_visible(&output, size, "开始压缩");
            }
            app.create_open = false;
            app.show_extract(false);
            if let Some(options) = &mut app.extraction {
                options.show_password = true;
            }
            for _ in 0..2 {
                draw(&mut app, &context, size);
            }
            let output = draw(&mut app, &context, size);
            assert_action_visible(&output, size, "开始解压");
            app.extraction = None;
            app.tasks_open = true;
            draw(&mut app, &context, size);
            app.tasks_open = false;
        }
    }
}
#[test]
fn archive_layout_preserves_selection_during_resize_and_theme_changes() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .select(0, false, false);
    for dark in [false, true] {
        app.preferences.dark = dark;
        theme::apply(&context, dark);
        for size in [[760.0, 480.0], [1000.0, 680.0], [1600.0, 1000.0]] {
            draw(&mut app, &context, size);
            let archive = app.archive.as_ref().unwrap();
            assert!(archive.browser.selection.contains("项目"));
            assert_eq!(archive.browser.visible.len(), 3);
        }
    }
}
#[test]
fn keyboard_range_selection_and_extract_shortcut_share_current_selection() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    draw(&mut app, &context, [1000.0, 680.0]);
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .select(0, false, false);
    context.memory_mut(|m| m.request_focus(egui::Id::new("archive-list")));
    key(
        &mut app,
        &context,
        egui::Key::ArrowDown,
        egui::Modifiers::SHIFT,
    );
    assert_eq!(app.archive.as_ref().unwrap().browser.selection.len(), 2);
    key(&mut app, &context, egui::Key::E, egui::Modifiers::COMMAND);
    let options = app.extraction.as_ref().unwrap();
    assert_eq!(
        options.selection,
        app.archive.as_ref().unwrap().browser.selection
    );
    assert!(!options.overwrite);
}
#[test]
fn escape_clears_search_without_cancelling_active_task() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    let browser = &mut app.archive.as_mut().unwrap().browser;
    browser.query = "notes".into();
    browser.refresh();
    browser.select(0, false, false);
    let (release, wait) = std::sync::mpsc::channel::<()>();
    app.start(&context, Kind::Extract, move |_| {
        let _ = wait.recv_timeout(Duration::from_secs(2));
        Err("test finished".into())
    });
    draw(&mut app, &context, [1000.0, 680.0]);
    key(&mut app, &context, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(app.archive.as_ref().unwrap().browser.query.is_empty());
    assert!(!app.active.as_ref().unwrap().job.control.is_cancelled());
    assert!(app
        .archive
        .as_ref()
        .unwrap()
        .browser
        .selection
        .contains("notes.txt"));
    drop(release);
}
#[test]
fn extraction_uses_a_selection_snapshot_and_default_destination() {
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .select(1, false, false);
    app.show_extract(false);
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .select(2, false, false);
    let options = app.extraction.as_ref().unwrap();
    assert_eq!(options.selection, BTreeSet::from(["notes.txt".into()]));
    assert_eq!(
        options.destination.path,
        engine::default_destination(&PathBuf::from("设计.zip"))
    );
}
#[test]
fn enter_submits_the_creation_form_through_password_validation() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.add_sources(vec!["source.txt".into()]);
    app.draft.show_password = true;
    draw(&mut app, &context, [760.0, 480.0]);
    key(&mut app, &context, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(app.form_error.as_ref().unwrap().contains("请设置密码"));
    assert!(!app.busy());
    app.draft.password = "secret".into();
    app.draft.confirm = "different".into();
    key(&mut app, &context, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(app.form_error.as_ref().unwrap().contains("两次密码不一致"));
    assert!(app.create_open);
}

#[test]
fn a_new_draft_does_not_reuse_sources_or_passwords_and_accepts_archive_sources() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.add_sources(vec!["old.txt".into()]);
    app.draft.password = "secret".into();
    app.new_archive();
    assert!(app.draft.sources.is_empty());
    assert!(app.draft.password.is_empty());
    app.accept_paths(&context, vec!["another.zip".into()]);
    assert_eq!(app.draft.sources, vec![PathBuf::from("another.zip")]);
    assert!(!app.busy());
}

fn finish_job(app: &mut App, context: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.busy() && Instant::now() < deadline {
        app.poll(context);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.busy(), "background operation did not finish");
}

#[test]
fn failed_creation_preserves_the_draft_without_retaining_its_password() {
    let context = context();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("来源.txt");
    let output = directory.path().join("已有.zip");
    std::fs::write(&source, "content").unwrap();
    std::fs::write(&output, "keep existing output").unwrap();
    let mut app = App::empty(Preferences::default());
    app.add_sources(vec![source.clone()]);
    app.draft.output.set(output.clone());
    app.draft.show_password = true;
    app.draft.password = "secret".into();
    app.draft.confirm = "secret".into();
    app.create_archive(&context);
    app.tasks_open = true;
    app.close_prompt = true;
    assert!(app.draft.password.is_empty() && app.draft.confirm.is_empty());
    finish_job(&mut app, &context);
    assert!(app.create_open && app.draft.show_password);
    assert!(!app.tasks_open && !app.close_prompt);
    assert_eq!(app.draft.sources, vec![source]);
    assert_eq!(app.draft.output.path, output);
    assert!(app.form_error.is_some());
    assert_eq!(
        std::fs::read_to_string(&output).unwrap(),
        "keep existing output"
    );
}

#[test]
fn failed_extraction_restores_the_target_and_selection_snapshot_for_retry() {
    let context = context();
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::empty(Preferences::default());
    let mut archive = archive();
    archive.path = directory.path().join("missing.zip");
    archive.browser.select(1, false, false);
    app.archive = Some(archive);
    app.show_extract(false);
    let destination = directory.path().join("output");
    let options = app.extraction.as_mut().unwrap();
    options.destination.set(destination.clone());
    options.overwrite = true;
    options.show_password = true;
    app.read_password = "secret".into();
    app.begin_extraction(&context);
    assert!(app.read_password.is_empty());
    app.archive.as_mut().unwrap().browser.clear_selection();
    finish_job(&mut app, &context);
    let options = app.extraction.as_ref().unwrap();
    assert_eq!(options.selection, BTreeSet::from(["notes.txt".into()]));
    assert_eq!(options.destination.path, destination);
    assert!(options.overwrite && options.show_password);
    assert!(app.form_error.is_some());
    assert!(app.read_password.is_empty() && app.pending_extraction.is_none());
    assert!(!destination.exists());

    app.extraction = None;
    app.quick_extract(&context, true);
    finish_job(&mut app, &context);
    let options = app.extraction.as_ref().unwrap();
    assert!(options.selection.is_empty());
    assert_eq!(options.destination.path, directory.path());
    assert!(!options.overwrite);
}

#[test]
fn enter_in_an_open_dropdown_does_not_submit_the_operation() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.add_sources(vec![PathBuf::from("来源.txt")]);
    for label in ["ZIP", "跳过已有文件"] {
        for _ in 0..2 {
            draw(&mut app, &context, [1000.0, 680.0]);
        }
        let output = draw(&mut app, &context, [1000.0, 680.0]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .expect("dropdown label missing");
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 680.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |context| app.render(context),
        );
        draw(&mut app, &context, [1000.0, 680.0]);
        assert!(egui::Popup::is_any_open(&context));
        key(&mut app, &context, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(!app.busy(), "Enter in a dropdown submitted the operation");
        assert!(if label == "ZIP" {
            app.create_open
        } else {
            app.extraction.is_some()
        });
        egui::Popup::close_all(&context);
        app.create_open = false;
        app.archive = Some(archive());
        app.show_extract(false);
    }
}

#[test]
fn menu_labels_and_shortcuts_have_separate_unclipped_columns() {
    for dark in [false, true] {
        for (size, os) in [
            ([760.0, 480.0], egui::os::OperatingSystem::Windows),
            ([1000.0, 680.0], egui::os::OperatingSystem::Windows),
            ([760.0, 480.0], egui::os::OperatingSystem::Mac),
            ([1000.0, 680.0], egui::os::OperatingSystem::Mac),
        ] {
            let context = context();
            context.set_os(os);
            theme::apply(&context, dark);
            let mut app = App::empty(Preferences {
                dark,
                ..Default::default()
            });
            app.archive = Some(archive());
            for (menu, label, shortcut_key) in
                [("文件", "新建压缩包…", 'N'), ("编辑", "搜索当前目录…", 'F')]
            {
                let output = draw(&mut app, &context, size);
                let position = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == menu => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap();
                let _ = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                        events: vec![
                            egui::Event::PointerMoved(position),
                            egui::Event::PointerButton {
                                pos: position,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers: egui::Modifiers::NONE,
                            },
                            egui::Event::PointerButton {
                                pos: position,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |context| app.render(context),
                );
                draw(&mut app, &context, size);
                let output = draw(&mut app, &context, size);
                let label = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some((shape.clip_rect, text))
                        }
                        _ => None,
                    })
                    .expect("menu action not displayed");
                let shortcut = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if text.galley.text().ends_with(shortcut_key)
                                && (text.galley.text().starts_with("Ctrl")
                                    || text.galley.text().starts_with("Cmd")
                                    || text.galley.text().contains('⌘')) =>
                        {
                            Some((shape.clip_rect, text))
                        }
                        _ => None,
                    })
                    .expect("menu shortcut not displayed");
                assert!(!shortcut.1.galley.text().contains('/'));
                assert_eq!(
                    shortcut.1.galley.text().starts_with("Ctrl"),
                    os == egui::os::OperatingSystem::Windows
                );
                let label_bounds = egui::Rect::from_min_size(label.1.pos, label.1.galley.size());
                let shortcut_bounds =
                    egui::Rect::from_min_size(shortcut.1.pos, shortcut.1.galley.size());
                assert!(
                    !label.1.galley.elided && !shortcut.1.galley.elided,
                    "menu text was truncated"
                );
                assert!(
                    shortcut_bounds.left() - label_bounds.right() >= 24.0,
                    "menu columns are crowded: {label_bounds:?}, {shortcut_bounds:?}"
                );
                assert!(
                    label.0.contains_rect(label_bounds)
                        && shortcut.0.contains_rect(shortcut_bounds)
                );
                app.archive
                    .as_mut()
                    .unwrap()
                    .browser
                    .select(0, false, false);
                key(&mut app, &context, egui::Key::Escape, egui::Modifiers::NONE);
                draw(&mut app, &context, size);
                assert!(
                    !egui::Popup::is_any_open(&context),
                    "Escape did not dismiss the menu"
                );
                assert!(
                    app.archive
                        .as_ref()
                        .unwrap()
                        .browser
                        .selection
                        .contains("项目"),
                    "dismissing a menu cleared the file selection"
                );
            }
        }
    }
}
