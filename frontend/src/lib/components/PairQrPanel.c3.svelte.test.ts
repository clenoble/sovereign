/** Pairing a device again, on the app's source side (spec ch. 05, PR1 and
 *  PR2; v0.0.13, item C3): when a device joins under a name a paired device
 *  already has, the pairing panel asks « Replace it » or « Keep both », and
 *  nothing is replaced without her answer. With several paired devices of
 *  that name, she picks one or keeps all.
 *
 *  The backend holds the question by the time `device-paired` reaches the
 *  screens (`sameNameQuestion`); her answer goes through `answerSameName`.
 *
 *  Written before the code by the test author of item C3; the implementer
 *  does not edit this file. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import type { SameNameQuestion } from '$lib/api/commands';

const h = vi.hoisted(() => ({
	question: null as SameNameQuestion | null,
	answers: [] as Array<string | null>
}));

vi.mock('qrcode', () => ({
	default: { toDataURL: vi.fn(async () => 'data:image/png;base64,') }
}));

vi.mock('$lib/api/commands', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api/commands')>();
	return {
		...actual,
		generatePairQr: vi.fn(async () => ({
			qr_payload_b64: 'offer',
			pin: 'ABCDE12345',
			expires_at: Date.now() + 300_000
		})),
		cancelPairing: vi.fn(async () => {}),
		sameNameQuestion: vi.fn(async () => h.question),
		answerSameName: vi.fn(async (replace: string | null) => {
			h.answers.push(replace);
		})
	};
});

import { pairing, onDevicePaired } from '$lib/stores/pairing.svelte';
import PairQrPanel from './PairQrPanel.svelte';

const OLD = { peer_id: '12D3KooWOldPixel', device_name: 'Pixel', paired_at: '2026-10-02T09:00:00Z' };

beforeEach(() => {
	h.question = null;
	h.answers = [];
	pairing.lastPaired = null;
	pairing.lastFailure = null;
	pairing.attemptsFailed = 0;
});

/** The panel, its offer shown, then the node reports that `peer` joined
 *  under `name`. */
async function joined(peer: string, name: string) {
	const onClose = vi.fn();
	const r = render(PairQrPanel, { props: { onClose } });
	await waitFor(() => r.getByAltText('Pairing QR code'));
	onDevicePaired(peer, name);
	return { ...r, onClose };
}

describe('PairQrPanel, a device joining under a name a paired device has', () => {
	it('asks « Replace it » or « Keep both », naming the device, and replaces nothing before she answers', async () => {
		h.question = { new_peer_id: '12D3KooWNewPixel', new_device_name: 'Pixel', candidates: [OLD] };
		const r = await joined('12D3KooWNewPixel', 'Pixel');
		await r.findByRole('button', { name: /Replace it/ });
		r.getByRole('button', { name: /Keep both/ });
		expect(document.body.textContent).toContain('Pixel');
		expect(h.answers, 'nothing is answered for her').toEqual([]);
	});

	it('« Replace it » answers with the paired device of that name', async () => {
		h.question = { new_peer_id: '12D3KooWNewPixel', new_device_name: 'Pixel', candidates: [OLD] };
		const r = await joined('12D3KooWNewPixel', 'Pixel');
		await fireEvent.click(await r.findByRole('button', { name: /Replace it/ }));
		await waitFor(() => expect(h.answers).toEqual(['12D3KooWOldPixel']));
	});

	it('« Keep both » replaces nothing', async () => {
		h.question = { new_peer_id: '12D3KooWNewPixel', new_device_name: 'Pixel', candidates: [OLD] };
		const r = await joined('12D3KooWNewPixel', 'Pixel');
		await fireEvent.click(await r.findByRole('button', { name: /Keep both/ }));
		await new Promise((res) => setTimeout(res, 20));
		expect(h.answers.filter((a) => a !== null), '« Keep both » names no device').toEqual([]);
	});

	it('closing the panel without an answer replaces nothing', async () => {
		h.question = { new_peer_id: '12D3KooWNewPixel', new_device_name: 'Pixel', candidates: [OLD] };
		const r = await joined('12D3KooWNewPixel', 'Pixel');
		await r.findByRole('button', { name: /Replace it/ });
		await fireEvent.click(r.getByRole('button', { name: 'Close' }));
		r.unmount();
		await new Promise((res) => setTimeout(res, 20));
		expect(h.answers.filter((a) => a !== null), 'no answer keeps both').toEqual([]);
	});

	it('with several paired devices of that name, she picks one and only it is named', async () => {
		const A = { peer_id: '12D3KooWPixelA', device_name: 'Pixel', paired_at: '2026-09-01T09:00:00Z' };
		const B = { peer_id: '12D3KooWPixelB', device_name: 'Pixel', paired_at: '2026-10-15T09:00:00Z' };
		h.question = { new_peer_id: '12D3KooWPixelNew', new_device_name: 'Pixel', candidates: [A, B] };
		const r = await joined('12D3KooWPixelNew', 'Pixel');
		await r.findAllByRole('button', { name: /Replace it/ });
		const radios = r.queryAllByRole('radio');
		if (radios.length > 0) {
			// One choice per device, then « Replace it ».
			expect(radios.length).toBe(2);
			await fireEvent.click(radios[1]);
			await fireEvent.click(r.getByRole('button', { name: /Replace it/ }));
		} else {
			// One « Replace it » per device, in the question's order.
			const replace = r.getAllByRole('button', { name: /Replace it/ });
			expect(replace.length).toBe(2);
			await fireEvent.click(replace[1]);
		}
		await waitFor(() => expect(h.answers).toEqual(['12D3KooWPixelB']));
	});

	it('a new name asks nothing', async () => {
		h.question = null;
		const r = await joined('12D3KooWTablet', 'Tablet');
		await r.findByText(/Paired with/);
		await new Promise((res) => setTimeout(res, 20));
		expect(r.queryByRole('button', { name: /Replace it/ })).toBeNull();
		expect(r.queryByRole('button', { name: /Keep both/ })).toBeNull();
	});
});
