/** C-SWEEP SW6 on the phone (spec ch. 09, lot LS(a)): the notice at login,
 *  in the Fable maille's words, in a `status` region (the Fable maille, by
 *  delegation of Céline, 2026-11-17); « OK » closes it. Whether it shows is
 *  the store's (`piiReview.svelte.test.ts`: it yields to the other notices).
 *
 *  Written before the code by the test author of lot LS(a); the implementer
 *  does not edit this file. */
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render } from '@testing-library/svelte';
import { tick } from 'svelte';
import { piiLoginNotice } from '$lib/stores/piiReview.svelte';
import PiiLoginNotice from './PiiLoginNotice.svelte';

const TITLE = 'Personal data to review';
const BODY = '3 more since your last login, 12 in all. They wait in the PII dashboard.';

beforeEach(() => {
	piiLoginNotice.title = null;
	piiLoginNotice.body = null;
});

describe('PiiLoginNotice', () => {
	it('shows nothing without a notice', () => {
		const { container } = render(PiiLoginNotice);
		expect(container.querySelector('[role="status"]')).toBeNull();
	});

	it('says the notice in a status region, then OK closes it', async () => {
		const { container, getByRole } = render(PiiLoginNotice);
		piiLoginNotice.title = TITLE;
		piiLoginNotice.body = BODY;
		await tick();
		const status = container.querySelector('[role="status"]');
		expect(status?.textContent).toContain(TITLE);
		expect(status?.textContent).toContain(BODY);
		await fireEvent.click(getByRole('button', { name: 'OK' }));
		await tick();
		expect(container.querySelector('[role="status"]')).toBeNull();
		expect(piiLoginNotice.title).toBeNull();
	});
});
