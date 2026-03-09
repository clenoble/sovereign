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

// Shared action gate helpers — used by both Chat quick-reply and ConfirmAction overlay.
// Import approveAction/rejectAction lazily to avoid circular dependencies.
export async function confirmPendingAction() {
	const { approveAction } = await import('$lib/api/commands');
	const { pushSystem } = await import('$lib/stores/chat.svelte');
	pushSystem('Approved.');
	app.pendingAction = null;
	try {
		await approveAction();
	} catch (e) {
		pushSystem(`Approve error: ${e}`);
	}
}

export async function rejectPendingAction(reason: string) {
	const { rejectAction } = await import('$lib/api/commands');
	const { pushSystem } = await import('$lib/stores/chat.svelte');
	pushSystem('Rejected.');
	app.pendingAction = null;
	try {
		await rejectAction(reason);
	} catch (e) {
		pushSystem(`Reject error: ${e}`);
	}
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
