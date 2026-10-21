/** The one-time migration's notice (C-DOC adoption step 6, M2; spec ch. 03,
 *  *Content encoding*, decided 2026-10-18 by Céline): at the first unlock
 *  after the update, documents are converted to format v2 in the background;
 *  when that is done the backend sends the `documents-migrated` event, once,
 *  with the text to show ("N documents converted to the new format — each
 *  one's earlier state is in its History."). The backend owns the wording
 *  (`sovereign_docs::MigrationNotice::text`); it names no document.
 *
 *  It stays until dismissed: it is shown once and never again. */

export const migrationNotice = $state({
	text: null as string | null
});

/** The `documents-migrated` event arrived. */
export function showMigrationNotice(text: string): void {
	migrationNotice.text = text.trim() === '' ? null : text;
}

/** The person closed the notice. */
export function dismissMigrationNotice(): void {
	migrationNotice.text = null;
}
