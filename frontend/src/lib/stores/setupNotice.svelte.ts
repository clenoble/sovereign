/** The notice after an interrupted setup (v0.0.12 P5g, contract `doc/plans/V0012-p5-wizard.md`).
 *  When the login screen opens with `setup_incomplete` true, the first successful login that
 *  follows shows this notice once, in the unlocked app. Never on the login screen: whoever looks
 *  at the locked phone learns nothing.
 *
 *  Nothing is stored by the frontend: this is memory only. A later launch does not show it,
 *  because the backend clears the marker at the first login of either persona and no longer
 *  reports `setup_incomplete`. */

export const SETUP_INCOMPLETE_TEXT =
	'Setup was interrupted before it finished. Your account works; your safety phrase, typing profile and sample documents may not have been saved.';

export const setupNotice = $state({
	visible: false
});

/** A login succeeded; `incomplete` is what the backend reported when the login screen opened. */
export function setSetupNoticeAfterLogin(incomplete: boolean): void {
	setupNotice.visible = incomplete;
}

/** The person closed the notice. */
export function dismissSetupNotice(): void {
	setupNotice.visible = false;
}
