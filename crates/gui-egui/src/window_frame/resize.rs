use eframe::egui::{self, viewport::ResizeDirection, Pos2, Rect};

/// Screen-space resize calculation for platforms without native drag-resize.
pub(super) struct ResizeDrag {
    original: Rect,
    pointer: Pos2,
    direction: ResizeDirection,
    minimum: [f32; 2],
}

impl ResizeDrag {
    pub(super) fn new(
        original: Rect,
        pointer: Pos2,
        direction: ResizeDirection,
        minimum: [f32; 2],
    ) -> Self {
        Self {
            original,
            pointer,
            direction,
            minimum,
        }
    }

    pub(super) fn begin(
        context: &egui::Context,
        direction: ResizeDirection,
        minimum: [f32; 2],
    ) -> Option<Self> {
        let (rect, pointer) = resize_input(context)?;
        Some(Self::new(rect, pointer, direction, minimum))
    }

    /// Return false after mouse release so the caller can drop the resize state.
    pub(super) fn update(&self, context: &egui::Context) -> bool {
        if !context.input(|input| input.pointer.primary_down()) {
            return false;
        }
        // A native window move changes its origin before a new local pointer position arrives.
        // Resizing on that stale position would repeatedly move the same edge by itself.
        let moved = context.input(|input| {
            input
                .raw
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerMoved(_)))
        });
        if !moved {
            return true;
        }
        if let Some((current, pointer)) = resize_input(context) {
            let next = self.rect(pointer);
            if (next.min - current.min).length() > 0.5 {
                context.send_viewport_cmd(egui::ViewportCommand::OuterPosition(next.min));
            }
            if (next.size() - current.size()).length() > 0.5 {
                context.send_viewport_cmd(egui::ViewportCommand::InnerSize(next.size()));
            }
        }
        true
    }

    pub(super) fn rect(&self, pointer: Pos2) -> Rect {
        use ResizeDirection::*;
        let delta = pointer - self.pointer;
        let mut rect = self.original;
        if matches!(self.direction, West | NorthWest | SouthWest) {
            rect.min.x = (rect.min.x + delta.x).min(rect.max.x - self.minimum[0]);
        }
        if matches!(self.direction, East | NorthEast | SouthEast) {
            rect.max.x = (rect.max.x + delta.x).max(rect.min.x + self.minimum[0]);
        }
        if matches!(self.direction, North | NorthWest | NorthEast) {
            rect.min.y = (rect.min.y + delta.y).min(rect.max.y - self.minimum[1]);
        }
        if matches!(self.direction, South | SouthWest | SouthEast) {
            rect.max.y = (rect.max.y + delta.y).max(rect.min.y + self.minimum[1]);
        }
        rect
    }
}

fn resize_input(context: &egui::Context) -> Option<(Rect, Pos2)> {
    context.input(|input| {
        let rect = input.viewport().inner_rect?;
        let pointer = rect.min + input.pointer.latest_pos()?.to_vec2();
        Some((rect, pointer))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_frame::MIN_INNER_SIZE;
    use eframe::egui::{pos2, vec2};

    fn draw(
        context: &egui::Context,
        rect: Rect,
        events: Vec<egui::Event>,
        render: impl FnMut(&egui::Context),
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, rect.size())),
            time: Some(context.input(|input| input.time) + 0.05),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .inner_rect = Some(rect);
        context.run(input, render)
    }

    #[test]
    fn native_origin_changes_do_not_resize_again_without_new_pointer_motion() {
        let context = egui::Context::default();
        let original = Rect::from_min_size(pos2(100.0, 100.0), vec2(1000.0, 680.0));
        let mut drag = None;
        let button = |pressed| egui::Event::PointerButton {
            pos: pos2(100.0, 100.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw(
            &context,
            original,
            vec![egui::Event::PointerMoved(pos2(100.0, 100.0)), button(true)],
            |context| {
                drag = ResizeDrag::begin(context, ResizeDirection::NorthWest, MIN_INNER_SIZE);
            },
        );
        let drag = drag.unwrap();
        let moved = Rect::from_min_max(pos2(140.0, 130.0), original.max);
        let output = draw(&context, moved, vec![], |context| {
            assert!(drag.update(context))
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .is_empty());
        let output = draw(
            &context,
            moved,
            vec![egui::Event::PointerMoved(pos2(110.0, 110.0))],
            |context| assert!(drag.update(context)),
        );
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.iter().any(|command| matches!(command, egui::ViewportCommand::OuterPosition(position) if *position == pos2(150.0, 140.0))));
        assert!(commands.iter().any(|command| matches!(command, egui::ViewportCommand::InnerSize(size) if *size == vec2(950.0, 640.0))));
        draw(&context, moved, vec![button(false)], |context| {
            assert!(!drag.update(context))
        });
    }

    #[test]
    fn resizing_keeps_the_opposite_edge_fixed_in_every_direction() {
        use ResizeDirection::*;
        let original = Rect::from_min_size(pos2(100.0, 100.0), vec2(1000.0, 680.0));
        let pointer = pos2(300.0, 200.0);
        for (direction, min, max) in [
            (NorthWest, pos2(140.0, 130.0), pos2(1100.0, 780.0)),
            (North, pos2(100.0, 130.0), pos2(1100.0, 780.0)),
            (NorthEast, pos2(100.0, 130.0), pos2(1140.0, 780.0)),
            (East, pos2(100.0, 100.0), pos2(1140.0, 780.0)),
            (SouthEast, pos2(100.0, 100.0), pos2(1140.0, 810.0)),
            (South, pos2(100.0, 100.0), pos2(1100.0, 810.0)),
            (SouthWest, pos2(140.0, 100.0), pos2(1100.0, 810.0)),
            (West, pos2(140.0, 100.0), pos2(1100.0, 780.0)),
        ] {
            let drag = ResizeDrag::new(original, pointer, direction, MIN_INNER_SIZE);
            assert_eq!(
                drag.rect(pointer + vec2(40.0, 30.0)),
                Rect::from_min_max(min, max),
                "{direction:?}"
            );
            for delta in [vec2(10000.0, 10000.0), vec2(-10000.0, -10000.0)] {
                let rect = drag.rect(pointer + delta);
                assert!(rect.width() >= MIN_INNER_SIZE[0], "{direction:?}: {rect:?}");
                assert!(
                    rect.height() >= MIN_INNER_SIZE[1],
                    "{direction:?}: {rect:?}"
                );
            }
        }
    }

    #[test]
    fn compact_windows_keep_their_own_minimum_in_every_resize_direction() {
        use ResizeDirection::*;
        let original = Rect::from_min_size(pos2(100.0, 100.0), vec2(560.0, 320.0));
        let pointer = pos2(300.0, 200.0);
        let minimum = crate::app::SHELL_WINDOW_MIN_SIZE;
        for direction in [
            NorthWest, North, NorthEast, East, SouthEast, South, SouthWest, West,
        ] {
            let drag = ResizeDrag::new(original, pointer, direction, minimum);
            assert_eq!(drag.rect(pointer), original, "{direction:?}");
            for delta in [vec2(10000.0, 10000.0), vec2(-10000.0, -10000.0)] {
                let rect = drag.rect(pointer + delta);
                assert!(
                    rect.width() >= minimum[0] && rect.height() >= minimum[1],
                    "{direction:?}: {rect:?}"
                );
                if matches!(direction, West | NorthWest | SouthWest) {
                    assert_eq!(rect.right(), original.right());
                }
                if matches!(direction, North | NorthWest | NorthEast) {
                    assert_eq!(rect.bottom(), original.bottom());
                }
            }
        }
    }
}
