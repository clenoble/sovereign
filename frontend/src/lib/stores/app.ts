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

/** Whether the model management panel is visible. */
export const modelPanelVisible = writable(false);

/** Whether the inbox panel is visible. */
export const inboxVisible = writable(false);

/** Contact panel state (null = closed). */
export const contactPanelState = writable<{ contactId: string } | null>(null);

// -- Phase 4 --

/** Authentication state machine. */
export type AuthState = 'checking' | 'onboarding' | 'login' | 'ready';
export const authState = writable<AuthState>('checking');

/** Whether the settings panel is visible. */
export const settingsVisible = writable(false);

/** Context menu state (null = closed). */
export interface ContextMenuState {
	x: number;
	y: number;
	docId: string;
	threadId: string;
}
export const contextMenu = writable<ContextMenuState | null>(null);
