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
    interaction::configure_pointer(&context);
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
    let bounds = text.1.galley.rect.translate(text.1.pos.to_vec2());
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

fn pointer_frame(
    app: &mut App,
    context: &egui::Context,
    position: egui::Pos2,
    button: Option<bool>,
    modifiers: egui::Modifiers,
) {
    let mut events = vec![egui::Event::PointerMoved(position)];
    if let Some(pressed) = button {
        events.push(egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    let time = context.input(|i| i.time) + 0.04;
    let _ = context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 680.0),
            )),
            time: Some(time),
            events,
            modifiers,
            ..Default::default()
        },
        |context| app.render(context),
    );
}
fn row_rect(context: &egui::Context, path: &str) -> egui::Rect {
    context
        .read_response(egui::Id::new((
            "archive-entry",
            &PathBuf::from("设计.zip"),
            path,
        )))
        .expect("row interaction missing")
        .rect
}
#[test]
fn floating_notices_keep_rows_stationary_and_double_click_opens_the_same_folder() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let before = row_rect(&context, "项目");
    let pos = before.left_center() + egui::vec2(70.0, 0.0);
    pointer_frame(&mut app, &context, pos, Some(true), egui::Modifiers::NONE);
    pointer_frame(&mut app, &context, pos, Some(false), egui::Modifiers::NONE);
    app.notice = Some(Notice::new("已解压完成", false));
    draw(&mut app, &context, [1000.0, 680.0]);
    let output = draw(&mut app, &context, [1000.0, 680.0]);
    assert_eq!(before, row_rect(&context, "项目"));
    let message = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "已解压完成" => Some(text.pos),
            _ => None,
        })
        .unwrap();
    assert!(
        message.x > 750.0 && message.y > 570.0 && message.y < 620.0,
        "notice is not above the footer on the right: {message:?}"
    );
    pointer_frame(&mut app, &context, pos, Some(true), egui::Modifiers::NONE);
    pointer_frame(&mut app, &context, pos, Some(false), egui::Modifiers::NONE);
    assert_eq!(app.archive.as_ref().unwrap().browser.directory, "项目");
}
fn notice_rect(context: &egui::Context) -> egui::Rect {
    context
        .memory(|memory| memory.area_rect(egui::Id::new("notice-toast")))
        .expect("notice area missing")
}

#[test]
fn floating_notices_fit_the_message_without_changing_the_workspace_or_footer() {
    for dark in [false, true] {
        for size in [[760.0, 480.0], [1000.0, 680.0]] {
            let context = context();
            theme::apply(&context, dark);
            let mut app = App::empty(Preferences {
                dark,
                ..Default::default()
            });
            app.archive = Some(archive());
            for _ in 0..2 {
                draw(&mut app, &context, size);
            }
            let rows = ["项目", "notes.txt", "other.txt"].map(|path| row_rect(&context, path));
            let workspace = context.available_rect();
            for (message, error) in [
                ("完成".to_string(), false),
                ("无法打开目标文件夹，请检查路径与访问权限。".repeat(6), true),
                ("完成".to_string(), false),
            ] {
                app.notice = Some(Notice::new(&message, error));
                // Check the first presented frame, including cached-size replacements.
                let output = draw(&mut app, &context, size);
                let rect = notice_rect(&context);
                assert!(rect.width() <= 421.0, "notice too wide: {rect:?}");
                if error {
                    assert!(rect.height() > 60.0, "long notice did not wrap: {rect:?}");
                } else {
                    assert!(
                        rect.width() < 180.0,
                        "short notice did not shrink: {rect:?}"
                    );
                }
                assert!((rect.right() - (size[0] - 12.0)).abs() < 1.0);
                assert!((rect.bottom() - (workspace.bottom() - 12.0)).abs() < 1.0);
                assert_eq!(context.available_rect(), workspace);
                assert_eq!(
                    ["项目", "notes.txt", "other.txt"].map(|path| row_rect(&context, path)),
                    rows
                );
                assert_action_visible(&output, size, &message);
                assert_action_visible(&output, size, "共 3 个条目 · 384 B");
            }
            app.notice = None;
            draw(&mut app, &context, size);
            assert_eq!(context.available_rect(), workspace);
        }
    }
}

fn timed_notice_frame(
    app: &mut App,
    context: &egui::Context,
    time: f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 680.0),
            )),
            time: Some(time),
            events,
            ..Default::default()
        },
        |context| app.render(context),
    )
}

#[test]
fn notices_expire_automatically_and_stationary_hover_restarts_the_full_timeout() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.notice = Some(Notice::new("已完成", false));
    timed_notice_frame(&mut app, &context, 0.0, vec![]);
    timed_notice_frame(&mut app, &context, 0.1, vec![]);
    timed_notice_frame(&mut app, &context, 1.9, vec![]);
    assert!(app.notice.is_some());
    timed_notice_frame(&mut app, &context, 2.1, vec![]);
    assert!(app.notice.is_none(), "notice did not expire while idle");

    app.notice = Some(Notice::new("新的提示", true));
    timed_notice_frame(&mut app, &context, 3.0, vec![]);
    let position = notice_rect(&context).center();
    timed_notice_frame(
        &mut app,
        &context,
        4.5,
        vec![egui::Event::PointerMoved(position)],
    );
    timed_notice_frame(&mut app, &context, 9.6, vec![]);
    assert!(
        app.notice.is_some(),
        "stationary hover failed to reset the timer"
    );
    timed_notice_frame(
        &mut app,
        &context,
        14.5,
        vec![egui::Event::PointerMoved(egui::pos2(100.0, 100.0))],
    );
    timed_notice_frame(&mut app, &context, 16.4, vec![]);
    assert!(app.notice.is_some(), "notice expired too soon after hover");
    timed_notice_frame(&mut app, &context, 16.6, vec![]);
    assert!(app.notice.is_none(), "notice did not expire after leaving");
}

#[test]
fn notice_close_button_dismisses_without_clearing_file_selection() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .select(0, false, false);
    app.notice = Some(Notice::new("已完成", false));
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let close = notice_rect(&context).right_center() - egui::vec2(29.0, 0.0);
    pointer_frame(&mut app, &context, close, Some(true), egui::Modifiers::NONE);
    pointer_frame(
        &mut app,
        &context,
        close,
        Some(false),
        egui::Modifiers::NONE,
    );
    assert!(app.notice.is_none());
    assert!(app
        .archive
        .as_ref()
        .unwrap()
        .browser
        .selection
        .contains("项目"));
}

#[test]
fn rectangle_selection_from_blank_space_and_ctrl_toggle_work_with_real_pointer_events() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let first = row_rect(&context, "项目");
    let last = row_rect(&context, "other.txt");
    let start = egui::pos2(last.right() - 20.0, last.bottom() + 35.0);
    let end = egui::pos2(first.right() - 100.0, first.center().y);
    for modifiers in [egui::Modifiers::NONE, egui::Modifiers::CTRL] {
        pointer_frame(&mut app, &context, start, Some(true), modifiers);
        pointer_frame(&mut app, &context, end, None, modifiers);
        pointer_frame(&mut app, &context, end, Some(false), modifiers);
        let browser = &app.archive.as_ref().unwrap().browser;
        if modifiers.ctrl {
            assert!(browser.selection.is_empty());
        } else {
            assert_eq!(browser.selection.len(), 3);
        }
        assert!(app.marquee.is_none());
        assert!(!app.busy(), "rectangle selection started an export");
    }
}
#[test]
fn dragging_a_selected_row_starts_metadata_only_without_reading_the_archive() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    app.archive.as_mut().unwrap().browser.select_all();
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let start = row_rect(&context, "notes.txt").left_center() + egui::vec2(80.0, 0.0);
    pointer_frame(&mut app, &context, start, Some(true), egui::Modifiers::NONE);
    pointer_frame(
        &mut app,
        &context,
        start + egui::vec2(20.0, 10.0),
        None,
        egui::Modifiers::NONE,
    );
    assert_eq!(app.archive.as_ref().unwrap().browser.selection.len(), 3);
    assert!(
        app.active.is_none(),
        "dragging started archive I/O before drop"
    );
    let pending = app
        .export
        .pending
        .as_ref()
        .expect("native drag was not scheduled");
    assert!(!pending.is_dropped());
    assert!(pending.control.snapshot().stage.is_empty());
    assert!(app.marquee.is_none());
}

#[test]
fn successive_double_clicks_enter_nested_directories_even_after_egui_triple_clicks() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    for path in ["项目", "项目/目录"] {
        for _ in 0..2 {
            draw(&mut app, &context, [1000.0, 680.0]);
        }
        let position = row_rect(&context, path).left_center() + egui::vec2(65.0, 0.0);
        for _ in 0..2 {
            pointer_frame(
                &mut app,
                &context,
                position,
                Some(true),
                egui::Modifiers::NONE,
            );
            pointer_frame(
                &mut app,
                &context,
                position,
                Some(false),
                egui::Modifiers::NONE,
            );
        }
        assert_eq!(app.archive.as_ref().unwrap().browser.directory, path);
    }
    key(
        &mut app,
        &context,
        egui::Key::Backspace,
        egui::Modifiers::NONE,
    );
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let position = row_rect(&context, "项目/目录").left_center() + egui::vec2(65.0, 0.0);
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(true),
        egui::Modifiers::NONE,
    );
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(false),
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.archive.as_ref().unwrap().browser.directory,
        "项目",
        "one click reopened a folder after returning"
    );
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(true),
        egui::Modifiers::NONE,
    );
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(false),
        egui::Modifiers::NONE,
    );
    assert_eq!(app.archive.as_ref().unwrap().browser.directory, "项目/目录");
}

fn primary_click(app: &mut App, context: &egui::Context, position: egui::Pos2) {
    pointer_frame(app, context, position, Some(true), egui::Modifiers::NONE);
    pointer_frame(app, context, position, Some(false), egui::Modifiers::NONE);
}

#[test]
fn folder_tree_expand_is_separate_from_navigation_and_preserves_list_selection() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .select(0, false, false);
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let id = egui::Id::new(("folder-tree-entry", &PathBuf::from("设计.zip"), "项目"));
    let expand = context
        .read_response(id.with("expand"))
        .unwrap()
        .rect
        .center();
    primary_click(&mut app, &context, expand);
    let browser = &app.archive.as_ref().unwrap().browser;
    assert_eq!(browser.directory, "");
    assert!(browser.selection.contains("项目"));
    assert!(browser
        .folder_tree
        .iter()
        .any(|node| node.path == "项目/目录"));
    draw(&mut app, &context, [1000.0, 680.0]);
    let child = context
        .read_response(egui::Id::new((
            "folder-tree-entry",
            &PathBuf::from("设计.zip"),
            "项目/目录",
        )))
        .unwrap()
        .rect
        .center();
    primary_click(&mut app, &context, child);
    let browser = &app.archive.as_ref().unwrap().browser;
    assert_eq!(browser.directory, "项目/目录");
    assert!(browser.selection.is_empty());
    assert!(browser.can_back());
    assert!(app.marquee.is_none());
}

#[test]
fn expanding_a_scrolled_tree_keeps_the_scroll_position_instead_of_jumping_to_root() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    let mut archive = archive();
    archive.entries = (0..40)
        .map(|i| Entry {
            path: format!("folder{i:02}/nested/file.txt"),
            size: 128,
            compressed_size: 64,
            crc32: None,
            modified: None,
            is_dir: false,
        })
        .collect();
    archive.browser = geezipx_egui::browser::Browser::new(&archive.entries);
    app.archive = Some(archive);
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    app.tree_offset = 500.0;
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let id = egui::Id::new(("folder-tree-entry", &PathBuf::from("设计.zip"), "folder20"));
    let expand = context
        .read_response(id.with("expand"))
        .unwrap()
        .rect
        .center();
    primary_click(&mut app, &context, expand);
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    assert!(
        app.tree_offset > 400.0,
        "expanding a folder jumped the tree to its selected root"
    );
    let browser = &app.archive.as_ref().unwrap().browser;
    assert!(browser.directory.is_empty());
    assert!(browser
        .folder_tree
        .iter()
        .any(|node| node.path == "folder20/nested"));
}

#[test]
fn parent_folder_row_stays_first_during_filtering_and_double_click_returns_with_child_focus() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .navigate("项目/目录".into());
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let parent = context
        .read_response(egui::Id::new((
            "archive-parent",
            &PathBuf::from("设计.zip"),
            "项目/目录",
        )))
        .unwrap()
        .rect;
    assert!(parent.bottom() <= row_rect(&context, "项目/目录/文件.txt").top());
    let point = parent.left_center() + egui::vec2(60.0, 0.0);
    primary_click(&mut app, &context, point);
    assert_eq!(app.archive.as_ref().unwrap().browser.directory, "项目/目录");
    assert!(app.archive.as_ref().unwrap().browser.selection.is_empty());
    primary_click(&mut app, &context, point);
    assert_eq!(app.archive.as_ref().unwrap().browser.directory, "项目");
    assert_eq!(
        app.archive.as_ref().unwrap().browser.focus.as_deref(),
        Some("项目/目录")
    );
    let browser = &mut app.archive.as_mut().unwrap().browser;
    browser.query = "unmatched".into();
    browser.refresh();
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let parent = context
        .read_response(egui::Id::new((
            "archive-parent",
            &PathBuf::from("设计.zip"),
            "项目",
        )))
        .unwrap()
        .rect;
    let point = parent.left_center() + egui::vec2(60.0, 0.0);
    primary_click(&mut app, &context, point);
    primary_click(&mut app, &context, point);
    let browser = &app.archive.as_ref().unwrap().browser;
    assert_eq!(browser.directory, "");
    assert!(browser.query.is_empty());
    assert_eq!(browser.focus.as_deref(), Some("项目"));
}

#[test]
fn rectangle_selection_with_parent_row_never_selects_or_exports_dot_dot() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    app.archive
        .as_mut()
        .unwrap()
        .browser
        .navigate("项目/目录".into());
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let file = row_rect(&context, "项目/目录/文件.txt");
    let start = egui::pos2(file.right() - 20.0, file.bottom() + 35.0);
    let end = egui::pos2(file.right() - 90.0, file.top() - 20.0);
    pointer_frame(&mut app, &context, start, Some(true), egui::Modifiers::NONE);
    pointer_frame(&mut app, &context, end, None, egui::Modifiers::NONE);
    pointer_frame(&mut app, &context, end, Some(false), egui::Modifiers::NONE);
    assert_eq!(
        app.archive.as_ref().unwrap().browser.selection,
        BTreeSet::from(["项目/目录/文件.txt".into()])
    );
    assert!(app.export.pending.is_none());
    assert!(!app.busy());
}

#[test]
fn folder_double_click_accepts_system_interval_above_egui_default() {
    let context = context();
    context.options_mut(|o| o.input_options.max_double_click_delay = 0.5);
    let mut app = App::empty(Preferences::default());
    app.archive = Some(archive());
    for _ in 0..2 {
        draw(&mut app, &context, [1000.0, 680.0]);
    }
    let position = row_rect(&context, "项目").left_center() + egui::vec2(65.0, 0.0);
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(true),
        egui::Modifiers::NONE,
    );
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(false),
        egui::Modifiers::NONE,
    );
    let time = context.input(|i| i.time) + 0.32;
    let _ = context.run(
        egui::RawInput {
            time: Some(time),
            ..Default::default()
        },
        |context| app.render(context),
    );
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(true),
        egui::Modifiers::NONE,
    );
    pointer_frame(
        &mut app,
        &context,
        position,
        Some(false),
        egui::Modifiers::NONE,
    );
    assert_eq!(app.archive.as_ref().unwrap().browser.directory, "项目");
}
