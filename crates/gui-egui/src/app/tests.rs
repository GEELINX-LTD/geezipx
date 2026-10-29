use super::*;
use geezipx_core::archive::Entry;

// Run real egui layout across changing viewport sizes. In particular, nested
// horizontal layouts used to calculate a negative width on the creation page.
#[test]
fn workspace_layout_survives_resize_and_format_changes() {
    let context = egui::Context::default();
    theme::fonts(&context);
    let mut app = App::empty(Preferences::default());
    for dark in [false, true] {
        app.preferences.dark = dark;
        theme::apply(&context, dark);
        for size in [[1180.0, 780.0], [920.0, 620.0], [1600.0, 1000.0]] {
            for page in [Page::Archive, Page::Create, Page::Tasks] {
                app.page = page;
                draw(&mut app, &context, size);
            }
            app.page = Page::Create;
            app.add_sources(vec![PathBuf::from("设计资源/说明.md")]);
            app.draft.password = "example".into();
            for format in 0..engine::formats::WRITE_FORMATS.len() {
                app.draft.format = format;
                draw(&mut app, &context, size);
            }
        }
    }
}

#[test]
fn archive_layout_preserves_selection_during_resize() {
    let entries = vec![Entry {
        path: "项目/目录/文件.txt".into(),
        size: 128,
        compressed_size: 64,
        crc32: None,
        modified: None,
        is_dir: false,
    }];
    let mut browser = geezipx_egui::browser::Browser::new(&entries);
    browser.selection.insert("项目".into());
    let context = egui::Context::default();
    theme::fonts(&context);
    let mut app = App::empty(Preferences::default());
    app.archive = Some(Archive {
        path: "设计.zip".into(),
        format: ArchiveFormat::Zip,
        browser,
        entries,
        bytes: 128,
    });
    for dark in [false, true] {
        theme::apply(&context, dark);
        for size in [[1180.0, 780.0], [920.0, 620.0]] {
            draw(&mut app, &context, size);
            let archive = app.archive.as_ref().unwrap();
            assert!(archive.browser.selection.contains("项目"));
            assert_eq!(archive.browser.visible.len(), 1);
        }
    }
}

fn draw(app: &mut App, context: &egui::Context, size: [f32; 2]) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
        ..Default::default()
    };
    let output = context.run(input, |context| {
        app.status_bar(context);
        app.sidebar(context);
        if app.page == Page::Archive && app.archive.is_some() {
            app.extract_panel(context);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(24))
            .show(context, |ui| match app.page {
                Page::Archive => app.workspace(ui, context),
                Page::Create => app.create_page(ui, context),
                Page::Tasks => app.tasks_page(ui),
            });
    });
    assert!(!output.shapes.is_empty());
}
