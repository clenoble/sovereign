/** Listen for Tauri events emitted by the Rust backend. */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { chat } from '$lib/stores/chat';
import {
	bubbleState,
	pendingAction,
	activeSuggestion,
	type BubbleState
} from '$lib/stores/app';

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

	// Return a combined unlisten function
	return () => {
		for (const fn of unlisteners) fn();
	};
}
