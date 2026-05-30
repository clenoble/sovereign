/** P2P sync state — Svelte 5 rune store.
 *
 * Listens to backend `sync-status`, `sync-conflict`, `device-discovered`,
 * and `device-paired` events (subscribed in `events.ts`) and exposes a
 * minimal status surface for the Taskbar indicator + Settings panel:
 *
 *   - inProgress: peer ids currently in an active sync session.
 *   - lastSyncedAt: ISO timestamp of the most recent successful completion.
 *   - lastError: the most recent error message (cleared by `clearError`).
 *   - conflicts: doc ids that surfaced a `sync-conflict` since the
 *     last user acknowledgement.
 *   - discoveredPeers: peer ids surfaced by mDNS but not yet paired.
 *
 * The "paired devices" list is a separate read-only fetch via
 * `listPairedDevices()` since it lives on disk; it isn't event-driven.
 */

export type SyncStatus = 'idle' | 'syncing' | 'error';

export const sync = $state({
	inProgress: new Set<string>(),
	lastSyncedAt: null as string | null,
	lastError: null as string | null,
	conflicts: [] as { docId: string; description: string }[],
	discoveredPeers: new Set<string>()
});

/** Compute a single high-level status for the Taskbar icon. */
export function syncStatus(): SyncStatus {
	if (sync.inProgress.size > 0) return 'syncing';
	if (sync.lastError) return 'error';
	return 'idle';
}

export function onSyncStarted(peerId: string) {
	sync.inProgress.add(peerId);
	// Trigger reactivity — Set mutation alone doesn't reactivate.
	sync.inProgress = new Set(sync.inProgress);
}

export function onSyncCompleted(peerId: string) {
	sync.inProgress.delete(peerId);
	sync.inProgress = new Set(sync.inProgress);
	sync.lastSyncedAt = new Date().toISOString();
	sync.lastError = null;
}

export function onSyncDisconnected(peerId: string) {
	// Disconnect doesn't necessarily mean failure — it just means the
	// peer left mDNS. Drop in-progress state if it was tracked.
	sync.inProgress.delete(peerId);
	sync.inProgress = new Set(sync.inProgress);
}

export function onSyncError(message: string) {
	sync.lastError = message;
}

export function onSyncConflict(docId: string, description: string) {
	if (!sync.conflicts.some((c) => c.docId === docId)) {
		sync.conflicts = [...sync.conflicts, { docId, description }];
	}
}

export function dismissConflict(docId: string) {
	sync.conflicts = sync.conflicts.filter((c) => c.docId !== docId);
}

export function clearError() {
	sync.lastError = null;
}

export function onDeviceDiscovered(peerId: string) {
	sync.discoveredPeers.add(peerId);
	sync.discoveredPeers = new Set(sync.discoveredPeers);
}
