//! K1 (v0.0.12, canary masked): the wizard's "show" button reveals the masked fields
//! when it is clicked. Found while the canary step was being masked: the button is drawn
//! inside its field's rectangle, and `hit_test` tested the field first, so a click on
//! "show" focused the field and the reveal was unreachable, on every masked step, since
//! the wizard was first drawn (a9af2ff, 2026-06-22). Written by PANDA2 (contract author);
//! the implementer does not edit it.

use super::*;

/// Window sizes the wizard is drawn at: a small laptop, a common desktop, a large one.
const SIZES: [(f64, f64); 3] = [(1024.0, 700.0), (1280.0, 800.0), (2560.0, 1440.0)];

fn wizard_at(step: usize) -> OnboardingWizard {
    let mut w = OnboardingWizard::new("Designation-7".into(), false);
    w.step = step;
    w
}

#[test]
fn k1_a_click_on_show_toggles_the_reveal_on_every_masked_step() {
    for step in [STEP_PASSWORD, STEP_DURESS, STEP_CANARY] {
        for (w, h) in SIZES {
            let wiz = wizard_at(step);
            let rects = field_rects(wiz_card(w, h), wiz.field_count());
            for (i, fr) in rects.iter().enumerate() {
                assert!(wiz.field_masked(i), "step {step}, field {i}: masked");
                let p = reveal_btn(*fr).center();
                assert_eq!(
                    wiz.hit_test(p, w, h),
                    WizAction::ToggleReveal,
                    "step {step}, field {i}, window {w}x{h}: a click on the drawn \"show\" button toggles the reveal (K1)"
                );
            }
        }
    }
}

#[test]
fn k1_a_click_elsewhere_in_a_masked_field_still_focuses_it() {
    for step in [STEP_PASSWORD, STEP_DURESS, STEP_CANARY] {
        for (w, h) in SIZES {
            let wiz = wizard_at(step);
            let rects = field_rects(wiz_card(w, h), wiz.field_count());
            for (i, fr) in rects.iter().enumerate() {
                let p = Point::new(fr.x0 + 12.0, fr.center().y);
                assert_eq!(
                    wiz.hit_test(p, w, h),
                    WizAction::FocusField(i),
                    "step {step}, field {i}, window {w}x{h}: a click on the text part focuses the field"
                );
            }
        }
    }
}

#[test]
fn k1_an_unmasked_field_has_no_show_button() {
    // The nickname step: a click where a "show" button would be drawn focuses the field.
    for (w, h) in SIZES {
        let wiz = wizard_at(STEP_NICKNAME);
        let rects = field_rects(wiz_card(w, h), wiz.field_count());
        for (i, fr) in rects.iter().enumerate() {
            assert!(!wiz.field_masked(i), "nickname step, field {i}: not masked");
            assert_eq!(wiz.hit_test(reveal_btn(*fr).center(), w, h), WizAction::FocusField(i), "nickname step, field {i}, window {w}x{h}");
        }
    }
}
