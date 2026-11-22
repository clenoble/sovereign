/** Listen for Tauri events emitted by the Rust backend. */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { chat, pushAssistant, pushSystem } from '$lib/stores/chat.svelte';
import { app, type BubbleState } from '$lib/stores/app.svelte';
import { openById } from '$lib/stores/documents.svelte';
import { refresh as canvasRefresh } from '$lib/stores/canvas.svelte';
import { refreshContacts } from '$lib/stores/contacts.svelte';
import {
	browser,
	setBrowserNavigated,
	setBrowserContentExtracted,
	setBrowserReliability,
	openBrowser as openBrowserStore,
	closeBrowser as closeBrowserStore
} from '$lib/stores/browser.svelte';
import { closeBrowserCmd } from '$lib/api/commands';
import { addSuggestion, removeSuggestion, type LinkSuggestion } from '$lib/stores/suggestions.svelte';
import { piiState, loadPii } from '$lib/stores/pii.svelte';
import { applyVoiceEvent, markSpeaking } from '$lib/stores/voice.svelte';
import {
	onDeviceDiscovered,
	onSyncCompleted,
	onSyncConflict,
	onSyncDisconnected,
	onSyncError,
	onSyncStarted
} from '$lib/stores/sync.svelte';
import { onDevicePaired, onPairingFailed } from '$lib/stores/pairing.svelte';
import {
	onPeerNeedsUpdate,
	onThisDeviceNeedsUpdate,
	onSyncRoundCompleted,
	onSyncRoundStarted,
	reloadSyncAlerts,
	reloadUnreadable
} from '$lib/stores/syncAlerts.svelte';
import { showMigrationNotice } from '$lib/stores/migrationNotice.svelte';
import { onSyncStopped } from '$lib/stores/syncStopped.svelte';
import { loadPiiSignal } from '$lib/stores/piiReview.svelte';
import type { PendingShare } from '$lib/stores/app.svelte';
import type { ReliabilityResultDto } from '$lib/api/commands';

// Payload types matching the Rust-side structs
interface ChatResponsePayload {
	text: string;
}
interface BubbleStatePayload {
	state: string;
}
interface ActionProposedPayload {
	action: string;
	level: string;
	description: string;
	doc_id: string | null;
	thread_id: string | null;
}
interface ActionExecutedPayload {
	action: string;
	success: boolean;
	/** The action's outcome, as the one line to show (only a real change is said as done). */
	summary: string;
}
/** A chat turn's explicit outcome (spec ch. 08), as the one line to show. */
interface ChatTurnEndedPayload {
	summary: string;
	/** The saved trace of a turn stopped at the round limit; nothing opens it by itself. */
	trace_doc_id: string | null;
	/** That trace's exact title, which the summary names (thread "Assistant traces"). */
	trace_title: string | null;
}
interface ActionRejectedPayload {
	action: string;
	reason: string;
}
interface SuggestionPayload {
	text: string;
	action: string;
}
interface SkillResultPayload {
	skill: string;
	action: string;
	kind: string;
	data: string;
}

/**
 * The chat text of a summary's `data` (`produce_summary`: `{doc_title,
 * summary, parts}`), the same as the desktop's: title and summary unchanged,
 * the part count only for more than one (`parts` missing or not a number
 * counts as 1). Unreadable data gives one plain line, never the raw text
 * (V0012-rc2-summary-shown, R3, R4).
 */
function summaryMessage(data: string): string {
	let v: unknown;
	try {
		v = JSON.parse(data);
	} catch {
		v = null;
	}
	const o = (v ?? {}) as Record<string, unknown>;
	if (typeof o.doc_title !== 'string' || typeof o.summary !== 'string') {
		return 'The summary could not be shown.';
	}
	const n = typeof o.parts === 'number' ? o.parts : 1;
	return n > 1
		? `Summary of '${o.doc_title}' (${n} parts):\n${o.summary}`
		: `Summary of '${o.doc_title}':\n${o.summary}`;
}
interface DocumentCreatedPayload {
	doc_id: string;
	title: string;
}
interface DocumentOpenedPayload {
	doc_id: string;
}
interface ThreadRenamedPayload {
	thread_id: string;
	name: string;
}
interface ThreadDeletedPayload {
	thread_id: string;
}
interface DocumentMovedPayload {
	doc_id: string;
	new_thread_id: string;
}
interface NewMessagesPayload {
	channel: string;
	count: number;
	conversation_id: string;
}
interface ContactCreatedPayload {
	contact_id: string;
	name: string;
}
interface ThreadMergedPayload {
	target_id: string;
	source_id: string;
}
interface ThreadSplitPayload {
	new_thread_id: string;
	name: string;
	doc_ids: string[];
}
interface InjectionDetectedPayload {
	source: string;
	indicators: string[];
	severity: number;
}
interface BrowserNavigatedPayload {
	url: string;
	title: string;
}
interface BrowserContentExtractedPayload {
	url: string;
	title: string;
	text: string;
}
interface ReliabilityAssessedPayload {
	doc_id: string;
	classification: string;
	score: number;
}
interface LinkSuggestedPayload {
	suggestion_id: string;
	from_doc_id: string;
	from_title: string;
	to_doc_id: string;
	to_title: string;
	relation_type: string;
	strength: number;
	rationale: string;
}
interface LinkSuggestionResolvedPayload {
	suggestion_id: string;
	accepted: boolean;
}
interface OpenPanelPayload {
	/** "pii_dashboard" | "models" | "inbox" | "browser" | "settings" */
	name: string;
}
interface VoiceEventPayload {
	/** "listening" | "transcription" | "speaking" | "idle" */
	kind: string;
	text?: string | null;
}
interface ShareReceivedPayload {
	content_type: 'text' | 'url';
	text?: string;
	url?: string;
	title?: string;
}

interface DeviceDiscoveredPayload {
	device_id: string;
	device_name: string;
}
interface SyncStatusPayload {
	peer_id: string;
	/** "started" | "completed (N items)" | "disconnected" | error message */
	status: string;
}
interface SyncConflictPayload {
	doc_id: string;
	description: string;
}
interface PeerNeedsUpdatePayload {
	peer_id: string;
	/** Empty when the device is not in the paired list. */
	device_name: string;
}
interface DevicePairedPayload {
	device_id: string;
	device_name: string;
}
interface PairingFailedPayload {
	reason: string;
	/** True when the armed offer self-destructed (expired or attempts
	 *  exhausted) and the pairing QR must be regenerated. */
	offer_dead: boolean;
}

/** Subscribe to all backend events. Returns an unlisten function. */
/** C-LOG DG3: set by `quarantined-log-diagnosed`, read by the reply that
 *  comes next. */
let logDiagnosed = false;

/** C-LOG DG3: the last chat message offers « Show what remains readable ». */
function markLastReadableLog() {
	const last = chat.messages.at(-1);
	if (!last) return;
	chat.messages = [...chat.messages.slice(0, -1), { ...last, offersReadableLog: true }];
}

export async function subscribeToEvents(): Promise<UnlistenFn> {
	logDiagnosed = false;
	const unlisteners: UnlistenFn[] = [];

	unlisteners.push(
		await listen<ChatResponsePayload>('chat-response', (e) => {
			pushAssistant(e.payload.text);
			// C-LOG DG3: the reply after the diagnosis offers the window.
			if (logDiagnosed) {
				logDiagnosed = false;
				markLastReadableLog();
			}
			// The robot speaks the reply via the Jiminy sidecar; light the mic
			// "speaking" indicator for the duration of delivery.
			markSpeaking(e.payload.text);
		})
	);

	unlisteners.push(
		await listen<BubbleStatePayload>('bubble-state', (e) => {
			app.bubbleState = e.payload.state as BubbleState;
		})
	);

	unlisteners.push(
		await listen<ActionProposedPayload>('action-proposed', (e) => {
			const p = e.payload;
			app.pendingAction = {
				action: p.action,
				level: p.level,
				description: p.description,
				docId: p.doc_id ?? undefined,
				threadId: p.thread_id ?? undefined
			};
			pushSystem(`Proposed: ${p.description}. Approve or reject?`);
		})
	);

	unlisteners.push(
		await listen<ActionExecutedPayload>('action-executed', (e) => {
			pushSystem(e.payload.summary);
			app.pendingAction = null;
		})
	);

	unlisteners.push(
		await listen<ChatTurnEndedPayload>('chat-turn-ended', (e) => {
			pushSystem(e.payload.summary);
			app.pendingAction = null;
		})
	);

	unlisteners.push(
		// C-LOG DG3: the diagnosis tool found a quarantine in this turn; its
		// reply, which comes next, offers the window.
		await listen('quarantined-log-diagnosed', () => {
			logDiagnosed = true;
		})
	);

	unlisteners.push(
		await listen<ActionRejectedPayload>('action-rejected', (e) => {
			pushSystem(`Rejected: ${e.payload.action} — ${e.payload.reason}`);
			// C-LOG DG3: the quarantine notice offers the window.
			if (e.payload.action === 'session log') markLastReadableLog();
			app.pendingAction = null;
		})
	);

	unlisteners.push(
		await listen<SuggestionPayload>('suggestion', (e) => {
			app.activeSuggestion = {
				text: e.payload.text,
				action: e.payload.action
			};
		})
	);

	unlisteners.push(
		await listen<SkillResultPayload>('skill-result', (e) => {
			// v0.0.12 rc2 (V0012-rc2-summary-shown, R3, R4): a summary is shown as
			// text, as on the desktop; other kinds stay as they were.
			if (e.payload.kind === 'summary') {
				pushAssistant(summaryMessage(e.payload.data));
				return;
			}
			pushSystem(`Skill "${e.payload.skill}": ${e.payload.data}`);
		})
	);

	unlisteners.push(
		await listen<DocumentCreatedPayload>('document-created', (e) => {
			openById(e.payload.doc_id);
			canvasRefresh();
		})
	);

	unlisteners.push(
		await listen<DocumentOpenedPayload>('document-opened', (e) => {
			openById(e.payload.doc_id);
		})
	);

	// Phase 3: Thread + canvas events
	unlisteners.push(
		await listen<ThreadRenamedPayload>('thread-renamed', () => {
			canvasRefresh();
		})
	);

	unlisteners.push(
		await listen<ThreadDeletedPayload>('thread-deleted', () => {
			canvasRefresh();
		})
	);

	unlisteners.push(
		await listen<DocumentMovedPayload>('document-moved', () => {
			canvasRefresh();
		})
	);

	unlisteners.push(
		await listen<ThreadMergedPayload>('thread-merged', () => {
			canvasRefresh();
		})
	);

	unlisteners.push(
		await listen<ThreadSplitPayload>('thread-split', () => {
			canvasRefresh();
		})
	);

	// Phase 3: Comms events
	unlisteners.push(
		await listen<NewMessagesPayload>('new-messages', () => {
			refreshContacts();
		})
	);

	unlisteners.push(
		await listen<ContactCreatedPayload>('contact-created', () => {
			refreshContacts();
		})
	);

	// Phase 5: Injection detection
	unlisteners.push(
		await listen<InjectionDetectedPayload>('injection-detected', (e) => {
			const p = e.payload;
			const severityLabel = p.severity >= 7 ? 'HIGH' : p.severity >= 4 ? 'MEDIUM' : 'LOW';
			const filtered = p.severity >= 7 ? ' Content was filtered for safety.' : '';
			pushSystem(
				`\u26a0\ufe0f Injection detected [${severityLabel}] in "${p.source}": ${p.indicators.join(', ')}.${filtered}`
			);
		})
	);

	// Browser events
	unlisteners.push(
		await listen<BrowserNavigatedPayload>('browser-navigated', (e) => {
			setBrowserNavigated(e.payload.url, e.payload.title);
		})
	);

	unlisteners.push(
		await listen<BrowserContentExtractedPayload>('browser-content-extracted', (e) => {
			setBrowserContentExtracted(e.payload.text);
		})
	);

	unlisteners.push(
		await listen<ReliabilityAssessedPayload>('reliability-assessed', (e) => {
			canvasRefresh();
		})
	);

	// Memory consolidation events
	unlisteners.push(
		await listen<LinkSuggestedPayload>('link-suggested', (e) => {
			const p = e.payload;
			addSuggestion({
				id: p.suggestion_id,
				fromDocId: p.from_doc_id,
				fromTitle: p.from_title,
				toDocId: p.to_doc_id,
				toTitle: p.to_title,
				relationType: p.relation_type,
				strength: p.strength,
				rationale: p.rationale,
				source: 'Consolidation'
			});
		})
	);

	unlisteners.push(
		await listen<LinkSuggestionResolvedPayload>('link-suggestion-resolved', (e) => {
			removeSuggestion(e.payload.suggestion_id);
		})
	);

	unlisteners.push(
		await listen<OpenPanelPayload>('open-panel', (e) => {
			// Mirror the toggle behaviour of the corresponding taskbar button.
			switch (e.payload.name) {
				case 'pii_dashboard':
					app.piiDashboardVisible = !app.piiDashboardVisible;
					if (app.piiDashboardVisible && !piiState.loaded) loadPii();
					break;
				case 'models':
					app.modelPanelVisible = !app.modelPanelVisible;
					break;
				case 'inbox':
					app.inboxVisible = !app.inboxVisible;
					break;
				case 'browser':
					if (browser.isOpen) {
						closeBrowserStore();
						closeBrowserCmd().catch(() => {});
					} else {
						openBrowserStore('https://duckduckgo.com');
					}
					break;
				case 'settings':
					app.settingsVisible = !app.settingsVisible;
					break;
				default:
					console.warn('open-panel: unknown panel name', e.payload.name);
			}
		})
	);

	// Voice pipeline state (listening / transcription / speaking / idle)
	unlisteners.push(
		await listen<VoiceEventPayload>('voice-event', (e) => {
			applyVoiceEvent(e.payload.kind, e.payload.text ?? undefined);
		})
	);

	// Mobile: OS share sheet delivers content here; SharePickerSheet handles it
	unlisteners.push(
		await listen<ShareReceivedPayload>('share-received', (e) => {
			const p = e.payload;
			app.pendingShare = {
				contentType: p.content_type,
				text: p.text,
				url: p.url,
				title: p.title
			} as PendingShare;
		})
	);

	// Phase 5: P2P sync events — feed the sync.svelte.ts store.
	unlisteners.push(
		await listen<DeviceDiscoveredPayload>('device-discovered', (e) => {
			onDeviceDiscovered(e.payload.device_id);
		})
	);
	unlisteners.push(
		await listen<SyncStatusPayload>('sync-status', (e) => {
			const { peer_id, status } = e.payload;
			if (status === 'started') {
				onSyncStarted(peer_id);
				onSyncRoundStarted(peer_id);
			} else if (status.startsWith('completed')) {
				onSyncCompleted(peer_id);
				onSyncRoundCompleted(peer_id);
				// A change received in this round may have raised an alert.
				reloadSyncAlerts();
				// C-SWEEP SW1, SW6: the backend swept what the round brought
				// before reporting it; the queue's count again.
				void loadPiiSignal().catch(() => {});
				// A paired peer just pushed rows into our DB. Refresh the
				// canvas + contacts so synced threads/docs/contacts appear
				// without a manual reload (the "it didn't land on mobile"
				// gap). The backend reports "completed (N items)"; skip the
				// refetch only when it explicitly synced zero items.
				const m = status.match(/\((\d+)\s+items?\)/);
				if (!m || parseInt(m[1], 10) > 0) {
					canvasRefresh();
					refreshContacts();
				}
			} else if (status === 'disconnected') {
				onSyncDisconnected(peer_id);
			} else {
				// Anything else surfaces as an error string.
				onSyncError(`${peer_id}: ${status}`);
			}
		})
	);
	unlisteners.push(
		await listen<SyncConflictPayload>('sync-conflict', (e) => {
			onSyncConflict(e.payload.doc_id, e.payload.description);
		})
	);
	// C-WIRE 12: a paired device on an older version (documents not exchanged).
	unlisteners.push(
		await listen<PeerNeedsUpdatePayload>('peer-needs-update', (e) => {
			onPeerNeedsUpdate(e.payload.peer_id, e.payload.device_name);
		})
	);
	// C-WIRE 12 from the other side: a paired device on a newer version, so
	// this device is the one to update (same payload: the newer device).
	unlisteners.push(
		await listen<PeerNeedsUpdatePayload>('this-device-needs-update', (e) => {
			onThisDeviceNeedsUpdate(e.payload.peer_id, e.payload.device_name);
		})
	);
	// C-WIRE 13: the background model verdict raised an alert.
	unlisteners.push(
		await listen('sync-alerts-changed', () => {
			reloadSyncAlerts();
		})
	);
	// C-SUP SU2: the node ended without the session asking for it; the
	// status line says « Sync stopped », and the notice shows once.
	unlisteners.push(
		await listen('sync-stopped', () => {
			onSyncStopped();
		})
	);
	// C-DOC adoption step 6 (M2): the one-time migration to document format
	// v2 completed; its notice, once (the backend owns the text).
	unlisteners.push(
		await listen<{ text: string }>('documents-migrated', (e) => {
			showMigrationNotice(e.payload.text);
			// v0.0.12 item 7 V2b, B5: a retry may have emptied the list of
			// documents that could not be read.
			void reloadUnreadable();
		})
	);
	unlisteners.push(
		await listen<DevicePairedPayload>('device-paired', (e) => {
			console.info('Device paired:', e.payload.device_id);
			onDevicePaired(e.payload.device_id, e.payload.device_name);
		})
	);
	unlisteners.push(
		await listen<PairingFailedPayload>('pairing-failed', (e) => {
			onPairingFailed(e.payload.reason, e.payload.offer_dead);
		})
	);
	// Guardian Access Recovery (F1) is poll-based — no event bridge. The
	// roster refreshes on a short timer while an enrollment offer is open,
	// and the recovery wizard drives access_recovery_poll on its own timer.

	// Return a combined unlisten function
	return () => {
		for (const fn of unlisteners) fn();
	};
}
