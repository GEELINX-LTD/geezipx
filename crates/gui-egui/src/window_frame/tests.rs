use super::*;

fn draw(
    window: &mut WindowFrame,
    context: &egui::Context,
    events: Vec<egui::Event>,
    modal: bool,
) -> (egui::FullOutput, bool) {
    let mut close = false;
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(760.0, 480.0),
        )),
        time: Some(context.input(|input| input.time) + 0.05),
        events,
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .inner_rect = input.screen_rect;
    let output = context.run(input, |context| {
        window.margin(context);
        if modal {
            egui::Modal::new(egui::Id::new("test-modal")).show(context, |ui| {
                ui.set_width(320.0);
                ui.label("Modal contents");
            });
        }
        close = window.show(context, false, "A long archive name.zip");
    });
    (output, close)
}

fn setup() -> (egui::Context, WindowFrame) {
    let context = egui::Context::default();
    let mut window = WindowFrame::default();
    for _ in 0..3 {
        draw(&mut window, &context, vec![], false);
    }
    (context, window)
}

fn pointer(position: egui::Pos2, pressed: Option<bool>) -> Vec<egui::Event> {
    let mut events = vec![egui::Event::PointerMoved(position)];
    if let Some(pressed) = pressed {
        events.push(egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    events
}

fn click(
    window: &mut WindowFrame,
    context: &egui::Context,
    position: egui::Pos2,
    modal: bool,
) -> (egui::FullOutput, bool) {
    draw(window, context, pointer(position, None), modal);
    draw(window, context, pointer(position, Some(true)), modal);
    draw(window, context, pointer(position, Some(false)), modal)
}

fn commands(output: &egui::FullOutput) -> &[egui::ViewportCommand] {
    &output.viewport_output[&egui::ViewportId::ROOT].commands
}

#[test]
fn restore_from_fullscreen_exits_fullscreen() {
    let (context, mut window) = setup();
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .fullscreen = Some(true);
    let output = context.run(input, |context| {
        assert_eq!(window.margin(context), 0);
        window.toggle_maximized(context);
    });
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Fullscreen(false))));
}

#[test]
fn controls_minimize_maximize_restore_and_request_close() {
    let header = Rect::from_min_size(egui::pos2(1.0, 1.0), egui::vec2(758.0, TITLE_HEIGHT));
    let (context, mut window) = setup();
    let (output, close) = click(
        &mut window,
        &context,
        control_rect(header, Control::Minimize).center(),
        false,
    );
    assert!(!close);
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Minimized(true))));
    let position = control_rect(header, Control::Maximize).center();
    let (output, _) = click(&mut window, &context, position, false);
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Maximized(true))));
    // An unacknowledged native maximize command must still toggle back to restore.
    let (output, _) = click(&mut window, &context, position, false);
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Maximized(false))));
    let (output, close) = click(
        &mut window,
        &context,
        control_rect(header, Control::Close).center(),
        false,
    );
    assert!(close);
    assert!(!commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Close)));
}

#[test]
fn title_clicks_preserve_double_click_and_movement_starts_native_drag() {
    let (context, mut window) = setup();
    let position = egui::pos2(400.0, 17.0);
    draw(&mut window, &context, pointer(position, None), false);
    let (output, _) = draw(&mut window, &context, pointer(position, Some(true)), false);
    assert!(!commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::StartDrag)));
    draw(&mut window, &context, pointer(position, Some(false)), false);
    let (output, _) = click(&mut window, &context, position, false);
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Maximized(true))));

    let (context, mut window) = setup();
    draw(&mut window, &context, pointer(position, None), false);
    draw(&mut window, &context, pointer(position, Some(true)), false);
    let (output, _) = draw(
        &mut window,
        &context,
        pointer(position + egui::vec2(12.0, 10.0), None),
        false,
    );
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::StartDrag)));
}

#[test]
fn other_controls_do_not_prime_title_double_clicks() {
    let (context, mut window) = setup();
    let header = Rect::from_min_size(egui::pos2(1.0, 1.0), egui::vec2(758.0, TITLE_HEIGHT));
    click(
        &mut window,
        &context,
        control_rect(header, Control::Maximize).center(),
        false,
    );
    assert!(window.maximized);
    let position = egui::pos2(150.0, 17.0); // On the archive-name label.
    let (output, _) = click(&mut window, &context, position, false);
    assert!(!commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Maximized(_))));
    let (output, _) = click(&mut window, &context, position, false);
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Maximized(false))));
}

#[test]
fn every_edge_and_corner_starts_the_matching_native_resize() {
    for (region, direction, cursor) in resize_regions(Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(760.0, 480.0),
    )) {
        let (context, mut window) = setup();
        let position = region.center();
        let (output, _) = draw(&mut window, &context, pointer(position, None), false);
        assert_eq!(output.platform_output.cursor_icon, cursor, "{direction:?}");
        let (output, _) = draw(&mut window, &context, pointer(position, Some(true)), false);
        #[cfg(not(target_os = "macos"))]
        assert!(commands(&output).iter().any(|command| matches!(command, egui::ViewportCommand::BeginResize(actual) if *actual == direction)), "{direction:?}: {:?}", commands(&output));
        #[cfg(target_os = "macos")]
        assert!(
            window.resize_drag.is_some(),
            "{direction:?}: {:?}",
            commands(&output)
        );
    }
}

#[test]
fn window_controls_work_above_modal_dialogs() {
    let (context, mut window) = setup();
    for _ in 0..3 {
        draw(&mut window, &context, vec![], true);
    }
    let header = Rect::from_min_size(egui::pos2(1.0, 1.0), egui::vec2(758.0, TITLE_HEIGHT));
    let (output, _) = click(
        &mut window,
        &context,
        control_rect(header, Control::Minimize).center(),
        true,
    );
    assert!(commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Minimized(true))));
    let (_, close) = click(
        &mut window,
        &context,
        control_rect(header, Control::Close).center(),
        true,
    );
    assert!(close);
}

#[test]
fn maximized_window_has_no_custom_resize_hotspots() {
    let (context, mut window) = setup();
    window.toggle_maximized(&context);
    for _ in 0..3 {
        draw(&mut window, &context, vec![], false);
    }
    assert_eq!(window.margin(&context), 0);
    let position = egui::pos2(2.0, 240.0);
    draw(&mut window, &context, pointer(position, None), false);
    let (output, _) = draw(&mut window, &context, pointer(position, Some(true)), false);
    assert!(!commands(&output)
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::BeginResize(_))));
}
