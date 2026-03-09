/** Listen for Tauri events emitted by the Rust backend. */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { pushAssistant, pushSystem } from '$lib/stores/chat.svelte';
import { app, type BubbleState } from '$lib/stores/app.svelte';
import { openById } from '$lib/stores/documents.svelte';
import { refresh as canvasRefresh } from '$lib/stores/canvas.svelte';
import { refreshContacts } from '$lib/stores/contacts.svelte';
import { setBrowserNavigated, setBrowserContentExtracted, setBrowserReliability } from '$lib/stores/browser.svelte';
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

	// Return a combined unlisten function
	return () => {
		for (const fn of unlisteners) fn();
	};
}
