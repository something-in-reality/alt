//! Scrolling reversed in place: the event keeps its source, timing, phase and acceleration, and
//! only its deltas change sign.

use objc2_core_graphics::{CGEvent, CGEventField};

/// Each axis's deltas: in lines, in fixed-point lines and in pixels.
const AXES: [[CGEventField; 3]; 2] = [
    [
        CGEventField::ScrollWheelEventDeltaAxis1,
        CGEventField::ScrollWheelEventFixedPtDeltaAxis1,
        CGEventField::ScrollWheelEventPointDeltaAxis1,
    ],
    [
        CGEventField::ScrollWheelEventDeltaAxis2,
        CGEventField::ScrollWheelEventFixedPtDeltaAxis2,
        CGEventField::ScrollWheelEventPointDeltaAxis2,
    ],
];

/// Reverses both axes of the scroll `event` in place. Setting an axis's line delta makes macOS
/// recompute its fixed-point and pixel deltas, so those are set after it, to their own reversed
/// values, which keeps the scroll smooth and accelerated.
pub fn reverse(event: &CGEvent) {
    let event = Some(event);
    for [lines, fixed, pixels] in AXES {
        let delta = (
            CGEvent::integer_value_field(event, lines),
            CGEvent::double_value_field(event, fixed),
            CGEvent::integer_value_field(event, pixels),
        );
        CGEvent::set_integer_value_field(event, lines, -delta.0);
        CGEvent::set_double_value_field(event, fixed, -delta.1);
        CGEvent::set_integer_value_field(event, pixels, -delta.2);
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_graphics::CGScrollEventUnit;

    use super::*;

    /// Every delta of an axis, in the order of `AXES`.
    fn deltas(event: &CGEvent) -> Vec<f64> {
        AXES.iter()
            .flat_map(|[lines, fixed, pixels]| {
                [
                    CGEvent::integer_value_field(Some(event), *lines) as f64,
                    CGEvent::double_value_field(Some(event), *fixed),
                    CGEvent::integer_value_field(Some(event), *pixels) as f64,
                ]
            })
            .collect()
    }

    #[test]
    fn reversing_negates_every_delta_of_both_axes() {
        let event = CGEvent::new_scroll_wheel_event2(None, CGScrollEventUnit::Line, 2, 3, -1, 0)
            .expect("a scroll event");
        let before = deltas(&event);
        assert!(before.iter().all(|delta| *delta != 0.0), "{before:?}");
        reverse(&event);
        let after = deltas(&event);
        assert_eq!(after, before.iter().map(|delta| -delta).collect::<Vec<_>>());
    }
}
