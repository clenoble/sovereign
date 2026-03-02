/** Listen for Tauri events emitted by the Rust backend. */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { chat } from '$lib/stores/chat';
import {
	bubbleState,
	pendingAction,
	activeSuggestion,
	type BubbleState
} from '$lib/stores/app';
import { documents } from '$lib/stores/documents';
import { canvas } from '$lib/stores/canvas';
import { contacts } from '$lib/stores/contacts';

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

/** Subscribe to all backend events. Returns an unlisten function. */
export async function subscribeToEvents(): Promise<UnlistenFn> {
	const unlisteners: UnlistenFn[] = [];

	unlisteners.push(
		await listen<ChatResponsePayload>('chat-response', (e) => {
			chat.pushAssistant(e.payload.text);
		})
	);

	unlisteners.push(
		await listen<BubbleStatePayload>('bubble-state', (e) => {
			bubbleState.set(e.payload.state as BubbleState);
		})
	);

	unlisteners.push(
		await listen<ActionProposedPayload>('action-proposed', (e) => {
			const p = e.payload;
			pendingAction.set({
				action: p.action,
				level: p.level,
				description: p.description,
				docId: p.doc_id ?? undefined,
				threadId: p.thread_id ?? undefined
			});
			chat.pushSystem(`Proposed: ${p.description}. Approve or reject?`);
		})
	);

	unlisteners.push(
		await listen<ActionExecutedPayload>('action-executed', (e) => {
			const msg = e.payload.success
				? `Done: ${e.payload.action}`
				: `Failed: ${e.payload.action}`;
			chat.pushSystem(msg);
			pendingAction.set(null);
		})
	);

	unlisteners.push(
		await listen<ActionRejectedPayload>('action-rejected', (e) => {
			chat.pushSystem(`Rejected: ${e.payload.action} — ${e.payload.reason}`);
			pendingAction.set(null);
		})
	);

	unlisteners.push(
		await listen<SuggestionPayload>('suggestion', (e) => {
			activeSuggestion.set({
				text: e.payload.text,
				action: e.payload.action
			});
		})
	);

	unlisteners.push(
		await listen<SkillResultPayload>('skill-result', (e) => {
			chat.pushSystem(`Skill "${e.payload.skill}": ${e.payload.data}`);
		})
	);

	unlisteners.push(
		await listen<DocumentCreatedPayload>('document-created', (e) => {
			documents.openById(e.payload.doc_id);
		})
	);

	unlisteners.push(
		await listen<DocumentOpenedPayload>('document-opened', (e) => {
			documents.openById(e.payload.doc_id);
		})
	);

	// Phase 3: Thread + canvas events
	unlisteners.push(
		await listen<ThreadRenamedPayload>('thread-renamed', () => {
			canvas.refresh();
		})
	);

	unlisteners.push(
		await listen<ThreadDeletedPayload>('thread-deleted', () => {
			canvas.refresh();
		})
	);

	unlisteners.push(
		await listen<DocumentMovedPayload>('document-moved', () => {
			canvas.refresh();
		})
	);

	unlisteners.push(
		await listen<ThreadMergedPayload>('thread-merged', () => {
			canvas.refresh();
		})
	);

	unlisteners.push(
		await listen<ThreadSplitPayload>('thread-split', () => {
			canvas.refresh();
		})
	);

	// Phase 3: Comms events
	unlisteners.push(
		await listen<NewMessagesPayload>('new-messages', () => {
			contacts.refresh();
		})
	);

	unlisteners.push(
		await listen<ContactCreatedPayload>('contact-created', () => {
			contacts.refresh();
		})
	);

	// Phase 5: Injection detection
	unlisteners.push(
		await listen<InjectionDetectedPayload>('injection-detected', (e) => {
			const p = e.payload;
			const severityLabel = p.severity >= 7 ? 'HIGH' : p.severity >= 4 ? 'MEDIUM' : 'LOW';
			const filtered = p.severity >= 7 ? ' Content was filtered for safety.' : '';
			chat.pushSystem(
				`\u26a0\ufe0f Injection detected [${severityLabel}] in "${p.source}": ${p.indicators.join(', ')}.${filtered}`
			);
		})
	);

	// Return a combined unlisten function
	return () => {
		for (const fn of unlisteners) fn();
	};
}
