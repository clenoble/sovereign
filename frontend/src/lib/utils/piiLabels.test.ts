/** C-SWEEP SW5 on the phone (spec ch. 09; Céline, 2026-11-17: « Étiquettes
 *  sur téléphone »): a token is shown as its type's label, never in its
 *  stored form `[pii:<id>]`, nor its value.
 *
 *  Written before the code by the test author of lot LS(a); the implementer
 *  does not edit this file. */
import { describe, expect, it } from 'vitest';
import { shownText, shownTitle } from './piiLabels';

const STORED_TITLE = 'Bank [pii:pii_record:abc123]';
const STORED_BODY = 'pay to [pii:pii_record:abc123] on Monday';

describe('the phone shows a token as its label', () => {
	it('shows the labelled form the backend computed', () => {
		expect(shownTitle(STORED_TITLE, 'Bank [IBAN]')).toBe('Bank [IBAN]');
		expect(shownText(STORED_BODY, 'pay to [IBAN] on Monday')).toBe('pay to [IBAN] on Monday');
	});

	it('without it, never the stored form: each token as the generic label', () => {
		for (const shown of [shownTitle(STORED_TITLE), shownText(STORED_BODY), shownText(STORED_BODY, null)]) {
			expect(shown).not.toContain('[pii:');
			expect(shown).toContain('[PII]');
		}
	});

	it('leaves a text without tokens as it is', () => {
		expect(shownTitle('Plain title')).toBe('Plain title');
		expect(shownText('plain text', undefined)).toBe('plain text');
	});
});
