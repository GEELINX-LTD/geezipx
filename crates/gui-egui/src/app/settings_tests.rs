use super::*;
use crate::app::tests::{assert_action_visible, click_text, context, draw};
use geezipx_egui::integration::Association;

fn snapshot(enabled: bool) -> Snapshot {
    Snapshot {
        associations: FORMATS
            .iter()
            .map(|format| Association {
                registered: enabled && format.available(),
                is_default: enabled && format.available(),
            })
            .collect(),
        menu: enabled,
    }
}
fn ready_settings() -> Settings {
    let mut settings = Settings {
        open: true,
        ..Default::default()
    };
    settings.accept_snapshot(snapshot(false), true);
    settings
}

#[test]
fn settings_preserve_drafts_on_refresh_and_failure_and_sync_after_apply() {
    let mut settings = ready_settings();
    settings.selected[0] = true;
    settings.menu = true;
    assert!(settings.dirty());
    settings.replace_draft = false; // Refresh while editing.
    settings.accept_snapshot(snapshot(false), true);
    assert!(settings.selected[0] && settings.menu && settings.dirty());
    settings.replace_draft = true; // Apply failed after partial registration.
    settings.accept_snapshot(snapshot(true), false);
    assert!(settings.selected[0] && settings.menu && settings.dirty());
    settings.accept_snapshot(snapshot(true), true);
    assert!(!settings.dirty());
    assert!(settings
        .selected
        .iter()
        .zip(FORMATS)
        .all(|(selected, f)| *selected == f.available()));
}

#[test]
fn settings_closing_requires_an_explicit_discard_of_system_changes() {
    let mut settings = ready_settings();
    settings.selected[0] = true;
    settings.request_close();
    assert!(settings.open);
    assert!(settings.confirmation == Some(Confirmation::Close));
    settings.confirmation = None;
    settings.restore_draft();
    assert!(!settings.dirty());
    settings.request_close();
    assert!(!settings.open);
}

#[test]
fn settings_actions_and_confirmation_fit_minimum_window_in_both_languages() {
    for language in Language::ALL {
        for dark in [false, true] {
            let context = context();
            theme::apply(&context, dark);
            let mut app = App::empty(Preferences {
                language,
                dark,
                ..Default::default()
            });
            app.settings = ready_settings();
            for page in [Page::Appearance, Page::Integration] {
                app.settings.page = page;
                for size in [[760.0, 480.0], [1000.0, 680.0]] {
                    for _ in 0..3 {
                        draw(&mut app, &context, size);
                    }
                    let output = draw(&mut app, &context, size);
                    assert_action_visible(&output, size, tr!(language, Done));
                    if page == Page::Appearance {
                        assert_action_visible(&output, size, tr!(language, Reset));
                    } else {
                        for label in [
                            tr!(language, ApplyIntegration),
                            tr!(language, RemoveIntegration),
                            tr!(language, SelectAllFormats),
                            tr!(language, SelectNoFormats),
                        ] {
                            assert_action_visible(&output, size, label);
                        }
                    }
                }
            }
            app.settings.selected[0] = true;
            click_text(&mut app, &context, tr!(language, Done));
            assert!(app.settings.open && app.settings.confirmation == Some(Confirmation::Close));
            for _ in 0..3 {
                draw(&mut app, &context, [760.0, 480.0]);
            }
            let output = draw(&mut app, &context, [760.0, 480.0]);
            assert_action_visible(&output, [760.0, 480.0], tr!(language, DiscardAndClose));
            click_text(&mut app, &context, tr!(language, Cancel));
            assert!(app.settings.open && app.settings.selected[0]);
            click_text(&mut app, &context, tr!(language, Done));
            click_text(&mut app, &context, tr!(language, DiscardAndClose));
            assert!(!app.settings.open && !app.settings.dirty());
        }
    }
}

#[test]
fn settings_format_search_and_page_switches_preserve_selected_formats() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.settings = ready_settings();
    click_text(&mut app, &context, "系统集成");
    click_text(&mut app, &context, "ZIP");
    assert!(app.settings.selected[0]);
    click_text(&mut app, &context, "外观");
    click_text(&mut app, &context, "深色");
    assert!(app.preferences.dark);
    click_text(&mut app, &context, "恢复默认");
    assert!(!app.preferences.dark);
    assert!(app.settings.selected[0] && app.settings.dirty());
    click_text(&mut app, &context, "系统集成");
    app.settings.format_search = ".TAR.GZ".into();
    let output = draw(&mut app, &context, [1000.0, 680.0]);
    let contains = |label: &str| {
        output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == label))
    };
    assert!(contains("TAR.GZ") && !contains("ZIP"));
    assert!(app.settings.selected[0]);
    click_text(&mut app, &context, "全选");
    assert!(app
        .settings
        .selected
        .iter()
        .zip(FORMATS)
        .all(|(selected, f)| *selected == f.available()));
    click_text(&mut app, &context, "全不选");
    assert!(app.settings.selected.iter().all(|selected| !selected));
}

#[test]
fn settings_removal_requires_confirmation_and_can_be_cancelled() {
    let context = context();
    let mut app = App::empty(Preferences::default());
    app.settings = ready_settings();
    app.settings.accept_snapshot(snapshot(true), true);
    app.settings.page = Page::Integration;
    click_text(&mut app, &context, "移除系统集成");
    assert!(app.settings.confirmation == Some(Confirmation::Remove));
    assert!(app.settings.integration_job.is_none());
    click_text(&mut app, &context, "取消");
    assert!(app.settings.confirmation.is_none());
    assert!(app.settings.has_integration() && !app.settings.dirty());
}
