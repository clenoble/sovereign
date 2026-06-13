/** Pairing-flow state (existing-device side) — Svelte 5 rune store.
 *
 * Fed by the `device-paired` and `pairing-failed` backend events
 * (subscribed in `events.ts`). The PairQrPanel watches this to react
 * live while the QR is on screen:
 *
 *   - lastPaired: the most recent successful pairing (new device's
 *     final peer id + name). Also signals the Settings panel to reload
 *     its paired-device list.
 *   - lastFailure: the most recent failed handshake attempt. When
 *     `offerDead` is true the armed offer self-destructed (wrong code
 *     three times, or expiry) and the panel must regenerate the QR.
 *   - attemptsFailed: count of failed attempts against the CURRENT
 *     offer (reset on regenerate/clear), for "2 of 3 attempts used"
 *     style feedback.
 */

export const pairing = $state({
	lastPaired: null as { peerId: string; deviceName: string; at: string } | null,
	lastFailure: null as { reason: string; offerDead: boolean } | null,
	attemptsFailed: 0
});

export function onDevicePaired(peerId: string, deviceName: string) {
	pairing.lastPaired = { peerId, deviceName, at: new Date().toISOString() };
	pairing.lastFailure = null;
	pairing.attemptsFailed = 0;
}

export function onPairingFailed(reason: string, offerDead: boolean) {
	pairing.lastFailure = { reason, offerDead };
	pairing.attemptsFailed += 1;
}

/** Reset transient status — called when the pairing panel (re)arms a
 *  fresh offer or closes. */
export function clearPairingStatus() {
	pairing.lastFailure = null;
	pairing.attemptsFailed = 0;
}
