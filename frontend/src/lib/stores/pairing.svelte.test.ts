import { beforeEach, describe, expect, it } from 'vitest';
import {
	pairing,
	onDevicePaired,
	onPairingFailed,
	clearPairingStatus
} from './pairing.svelte';

beforeEach(() => {
	pairing.lastPaired = null;
	pairing.lastFailure = null;
	pairing.attemptsFailed = 0;
});

describe('pairing store', () => {
	it('records a successful pairing and clears any failure state', () => {
		onPairingFailed('wrong pairing code', false);
		expect(pairing.attemptsFailed).toBe(1);

		onDevicePaired('12D3KooWNewPhone', 'New phone');
		expect(pairing.lastPaired?.peerId).toBe('12D3KooWNewPhone');
		expect(pairing.lastPaired?.deviceName).toBe('New phone');
		expect(pairing.lastPaired?.at).toBeTruthy();
		expect(pairing.lastFailure).toBeNull();
		expect(pairing.attemptsFailed).toBe(0);
	});

	it('counts failed attempts against the current offer', () => {
		onPairingFailed('wrong pairing code', false);
		onPairingFailed('wrong pairing code', false);
		expect(pairing.attemptsFailed).toBe(2);
		expect(pairing.lastFailure?.offerDead).toBe(false);

		// Third strike kills the offer.
		onPairingFailed('wrong pairing code', true);
		expect(pairing.attemptsFailed).toBe(3);
		expect(pairing.lastFailure?.offerDead).toBe(true);
	});

	it('clearPairingStatus resets failures but keeps the last success', () => {
		onDevicePaired('12D3KooWNewPhone', 'New phone');
		onPairingFailed('pairing offer expired', true);

		clearPairingStatus();
		expect(pairing.lastFailure).toBeNull();
		expect(pairing.attemptsFailed).toBe(0);
		// The success record survives — Settings uses it to refresh the
		// device list even after the panel re-arms a new offer.
		expect(pairing.lastPaired?.deviceName).toBe('New phone');
	});
});
