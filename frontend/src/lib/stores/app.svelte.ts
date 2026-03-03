/** Rune-based reactive state for global app UI. */

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

export type AuthState = 'checking' | 'onboarding' | 'login' | 'ready';

export interface ContextMenuState {
	x: number;
	y: number;
	docId: string;
	threadId: string;
}

/** Reactive app state — $state() creates a deep Proxy for fine-grained tracking. */
export const app = $state({
	bubbleState: 'Idle' as BubbleState,
	pendingAction: null as PendingAction | null,
	activeSuggestion: null as Suggestion | null,
	searchVisible: false,
	orchestratorAvailable: false,
	modelPanelVisible: false,
	inboxVisible: false,
	contactPanelState: null as { contactId: string; conversationId?: string } | null,
	authState: 'checking' as AuthState,
	settingsVisible: false,
	contextMenu: null as ContextMenuState | null,
	skillsPanelVisible: false,
	bubbleStyle: 'icon' as string
});
