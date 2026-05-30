/** Listen for Tauri events emitted by the Rust backend. */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { pushAssistant, pushSystem } from '$lib/stores/chat.svelte';
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
import { applyVoiceEvent } from '$lib/stores/voice.svelte';
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

/** Subscribe to all backend events. Returns an unlisten function. */
export async function subscribeToEvents(): Promise<UnlistenFn> {
	const unlisteners: UnlistenFn[] = [];

	unlisteners.push(
		await listen<ChatResponsePayload>('chat-response', (e) => {
			pushAssistant(e.payload.text);
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
			const msg = e.payload.success
				? `Done: ${e.payload.action}`
				: `Failed: ${e.payload.action}`;
			pushSystem(msg);
			app.pendingAction = null;
		})
	);

	unlisteners.push(
		await listen<ActionRejectedPayload>('action-rejected', (e) => {
			pushSystem(`Rejected: ${e.payload.action} — ${e.payload.reason}`);
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

	// Return a combined unlisten function
	return () => {
		for (const fn of unlisteners) fn();
	};
}
