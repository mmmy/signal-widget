#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetHitZone {
    Transparent,
    Drag,
}

pub fn point_in_circle(point: (f32, f32), center: (f32, f32), radius: f32) -> bool {
    let dx = point.0 - center.0;
    let dy = point.1 - center.1;
    (dx * dx) + (dy * dy) <= radius * radius
}

pub fn classify_hit(point: (f32, f32), center: (f32, f32), radius: f32) -> WidgetHitZone {
    if point_in_circle(point, center, radius) {
        WidgetHitZone::Drag
    } else {
        WidgetHitZone::Transparent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_inside_circle_returns_true() {
        assert!(point_in_circle((28.0, 28.0), (28.0, 28.0), 28.0));
        assert!(point_in_circle((40.0, 28.0), (28.0, 28.0), 28.0));
    }

    #[test]
    fn point_outside_circle_returns_false() {
        assert!(!point_in_circle((0.0, 0.0), (28.0, 28.0), 20.0));
        assert!(!point_in_circle((60.0, 28.0), (28.0, 28.0), 20.0));
    }

    #[test]
    fn classify_hit_returns_drag_for_inside_and_transparent_for_outside() {
        assert_eq!(
            classify_hit((28.0, 28.0), (28.0, 28.0), 20.0),
            WidgetHitZone::Drag
        );
        assert_eq!(
            classify_hit((0.0, 0.0), (28.0, 28.0), 20.0),
            WidgetHitZone::Transparent
        );
    }
}
