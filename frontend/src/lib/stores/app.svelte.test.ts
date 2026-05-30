import { beforeEach, describe, expect, it } from 'vitest';
import {
	app,
	confirmPendingAction,
	rejectPendingAction
} from './app.svelte';
import { chat } from './chat.svelte';
import { mockTauriCommand } from '$lib/test/tauri';

beforeEach(() => {
	app.bubbleState = 'Idle';
	app.pendingAction = null;
	app.activeSuggestion = null;
	app.searchVisible = false;
	app.orchestratorAvailable = false;
	app.modelPanelVisible = false;
	app.inboxVisible = false;
	app.contactPanelState = null;
	app.authState = 'checking';
	app.settingsVisible = false;
	app.contextMenu = null;
	app.skillsPanelVisible = false;
	app.bubbleStyle = 'icon';

	chat.messages = [];
	chat.generating = false;
	chat.input = '';
	chat.visible = false;
});

describe('app store — initial defaults', () => {
	it('starts in checking auth state', () => {
		expect(app.authState).toBe('checking');
	});

	it('has no pending action by default', () => {
		expect(app.pendingAction).toBeNull();
	});

	it('all panels start hidden', () => {
		expect(app.searchVisible).toBe(false);
		expect(app.modelPanelVisible).toBe(false);
		expect(app.inboxVisible).toBe(false);
		expect(app.settingsVisible).toBe(false);
		expect(app.skillsPanelVisible).toBe(false);
	});

	it('bubble starts Idle with default style "icon"', () => {
		expect(app.bubbleState).toBe('Idle');
		expect(app.bubbleStyle).toBe('icon');
	});
});

describe('confirmPendingAction', () => {
	it('clears pendingAction, pushes "Approved." system message, and invokes approve_action', async () => {
		const invocations: string[] = [];
		mockTauriCommand('approve_action', () => {
			invocations.push('approve_action');
			return undefined;
		});

		app.pendingAction = {
			action: 'create_doc',
			level: 'Modify',
			description: 'Create new document'
		};

		await confirmPendingAction();

		expect(app.pendingAction).toBeNull();
		expect(invocations).toEqual(['approve_action']);
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].role).toBe('system');
		expect(chat.messages[0].text).toBe('Approved.');
	});

	it('clears pendingAction BEFORE invoking — even if the backend call throws, state stays cleared', async () => {
		mockTauriCommand('approve_action', () => {
			throw new Error('backend unavailable');
		});

		app.pendingAction = {
			action: 'x',
			level: 'y',
			description: 'z'
		};

		await confirmPendingAction();

		expect(app.pendingAction).toBeNull();
		// Both messages: "Approved." (optimistic), then "Approve error: ..."
		const texts = chat.messages.map((m) => m.text);
		expect(texts).toContain('Approved.');
		expect(
			texts.some((t) => t.startsWith('Approve error:') && t.includes('backend unavailable'))
		).toBe(true);
	});

	it('does not throw even when approveAction rejects', async () => {
		mockTauriCommand('approve_action', () => {
			throw new Error('boom');
		});
		app.pendingAction = { action: 'a', level: 'b', description: 'c' };

		// Should resolve (the function catches internally)
		await expect(confirmPendingAction()).resolves.toBeUndefined();
	});
});

describe('rejectPendingAction', () => {
	it('clears pendingAction, pushes "Rejected." system message, and forwards the reason', async () => {
		let receivedArgs: unknown = null;
		mockTauriCommand('reject_action', (args) => {
			receivedArgs = args;
			return undefined;
		});

		app.pendingAction = { action: 'x', level: 'y', description: 'z' };

		await rejectPendingAction('user said no');

		expect(app.pendingAction).toBeNull();
		expect(receivedArgs).toEqual({ reason: 'user said no' });
		expect(chat.messages.some((m) => m.text === 'Rejected.')).toBe(true);
	});

	it('handles backend errors with a "Reject error:" system message and keeps pendingAction cleared', async () => {
		mockTauriCommand('reject_action', () => {
			throw new Error('rejection failed');
		});
		app.pendingAction = { action: 'x', level: 'y', description: 'z' };

		await rejectPendingAction('whatever');

		expect(app.pendingAction).toBeNull();
		const texts = chat.messages.map((m) => m.text);
		expect(texts).toContain('Rejected.');
		expect(
			texts.some((t) => t.startsWith('Reject error:') && t.includes('rejection failed'))
		).toBe(true);
	});

	it('forwards an empty reason as-is', async () => {
		let receivedArgs: unknown = null;
		mockTauriCommand('reject_action', (args) => {
			receivedArgs = args;
			return undefined;
		});
		app.pendingAction = { action: 'a', level: 'b', description: 'c' };

		await rejectPendingAction('');

		expect(receivedArgs).toEqual({ reason: '' });
	});
});
