/** C-SWEEP on the phone (spec ch. 09, lot LS(a)): the review's bindings call
 *  their commands with their arguments. Written before the code by the test
 *  author of lot LS(a); the implementer does not edit this file. */
import { describe, expect, it } from 'vitest';
import { mockTauriCommand } from '$lib/test/tauri';
import {
	getDocumentLabelled,
	getPiiSignal,
	listPiiReviewGroups,
	reviewPiiRecords,
	takePiiLoginNotice
} from './commands';

describe('C-SWEEP bindings', () => {
	it('lists the review groups', async () => {
		mockTauriCommand('pii_review_groups', () => [{ kind: 'iban', label: 'IBAN', rows: [] }]);
		expect(await listPiiReviewGroups()).toEqual([{ kind: 'iban', label: 'IBAN', rows: [] }]);
	});

	it('sends a gesture on the records selected, and nothing that names a type', async () => {
		let seen: Record<string, unknown> | null = null;
		mockTauriCommand('pii_review', (args: Record<string, unknown>) => {
			seen = args;
			return { rewritten: ['document:a'] };
		});
		expect(await reviewPiiRecords('dismiss', ['pii_record:1', 'pii_record:2'])).toEqual({ rewritten: ['document:a'] });
		expect(seen).toEqual({ gesture: 'dismiss', recordIds: ['pii_record:1', 'pii_record:2'] });
	});

	it('reads the signal, the notice at login and a labelled document', async () => {
		mockTauriCommand('pii_signal', () => ({ waiting: 3, status: '3 personal data to review' }));
		mockTauriCommand('pii_login_notice', () => null);
		let asked: unknown = null;
		mockTauriCommand('get_document_labelled', (args: { id: string }) => {
			asked = args.id;
			return { title: 'Bank', body: 'pay to [IBAN]' };
		});
		expect(await getPiiSignal()).toEqual({ waiting: 3, status: '3 personal data to review' });
		expect(await takePiiLoginNotice()).toBeNull();
		expect(await getDocumentLabelled('document:a')).toEqual({ title: 'Bank', body: 'pay to [IBAN]' });
		expect(asked).toBe('document:a');
	});
});
