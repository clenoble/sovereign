import { writable } from 'svelte/store';

export type BubbleState =
	| 'Idle'
	| 'ProcessingOwned'
	| 'ProcessingExternal'
	| 'Proposing'
	| 'Executing'
	| 'Suggesting';

export interface PendingAction {
	action: string;
	level: string;
	description: string;
	docId?: string;
	threadId?: string;
}

export interface Suggestion {
	text: string;
	action: string;
}

/** AI bubble visual state. */
export const bubbleState = writable<BubbleState>('Idle');

/** Currently pending action awaiting user approval. */
export const pendingAction = writable<PendingAction | null>(null);

/** Active proactive suggestion. */
export const activeSuggestion = writable<Suggestion | null>(null);

/** Whether the search overlay is visible. */
export const searchVisible = writable(false);

/** Whether the orchestrator is available. */
export const orchestratorAvailable = writable(false);
