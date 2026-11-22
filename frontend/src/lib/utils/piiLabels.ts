/** C-SWEEP SW5 (spec ch. 09; Céline, 2026-11-17, for the phone: « Étiquettes
 *  sur téléphone »): what the phone shows of a document's title and text,
 *  each personal-data token as its type's label, never the stored form
 *  `[pii:<id>]`.
 *
 *  Given the labelled form the backend computed (`labelled_title`,
 *  `labelled_body`, by the resolver rule), that is what is shown. Without it,
 *  each token form shows as the generic label `[PII]`: in doubt, the stored
 *  form is not shown (as the shell's `shown_title` does).
 *
 *  Signatures by the test author of lot LS(a); the bodies are the
 *  implementer's. */

const TOKEN = /\[pii:[^\]]*\]/g;
const GENERIC = '[PII]';

function shown(stored: string, labelled?: string | null): string {
	if (typeof labelled === 'string') return labelled;
	return stored.replace(TOKEN, GENERIC);
}

/** What the phone shows of a title. */
export function shownTitle(stored: string, labelled?: string | null): string {
	return shown(stored, labelled);
}

/** What the phone shows of a document's text. */
export function shownText(stored: string, labelled?: string | null): string {
	return shown(stored, labelled);
}
