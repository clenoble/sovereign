import { beforeEach, describe, expect, it } from 'vitest';
import {
	chat,
	clearGenerating,
	pushAssistant,
	pushSystem,
	pushUser,
	recentMessages,
	toggleChat,
	type ChatMessage
} from './chat.svelte';

beforeEach(() => {
	chat.messages = [];
	chat.generating = false;
	chat.input = '';
	chat.visible = false;
});

describe('pushUser', () => {
	it('appends a user message with the given text', () => {
		pushUser('hello');
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].role).toBe('user');
		expect(chat.messages[0].text).toBe('hello');
	});

	it('stamps a recent timestamp', () => {
		const before = Date.now();
		pushUser('hi');
		const after = Date.now();
		const ts = chat.messages[0].timestamp;
		expect(ts).toBeGreaterThanOrEqual(before);
		expect(ts).toBeLessThanOrEqual(after);
	});

	it('flips generating to true (signals a reply is in flight)', () => {
		expect(chat.generating).toBe(false);
		pushUser('go');
		expect(chat.generating).toBe(true);
	});

	it('appends to existing messages without dropping prior ones', () => {
		pushUser('first');
		pushUser('second');
		expect(chat.messages.map((m) => m.text)).toEqual(['first', 'second']);
	});
});

describe('pushAssistant', () => {
	it('appends an assistant message and clears generating', () => {
		chat.generating = true;
		pushAssistant('reply');
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].role).toBe('assistant');
		expect(chat.messages[0].text).toBe('reply');
		expect(chat.generating).toBe(false);
	});

	it('completes a typical user → assistant turn', () => {
		pushUser('q');
		expect(chat.generating).toBe(true);
		pushAssistant('a');
		expect(chat.generating).toBe(false);
		expect(chat.messages.map((m) => m.role)).toEqual(['user', 'assistant']);
	});
});

describe('pushSystem', () => {
	it('appends a system message', () => {
		pushSystem('connected');
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].role).toBe('system');
	});

	it('does NOT change generating (regression: system messages must not interfere with reply state)', () => {
		chat.generating = true;
		pushSystem('warning');
		expect(chat.generating).toBe(true);

		chat.generating = false;
		pushSystem('info');
		expect(chat.generating).toBe(false);
	});
});

describe('clearGenerating', () => {
	it('forces generating to false', () => {
		chat.generating = true;
		clearGenerating();
		expect(chat.generating).toBe(false);
	});

	it('is a no-op when already false', () => {
		chat.generating = false;
		clearGenerating();
		expect(chat.generating).toBe(false);
	});
});

describe('toggleChat', () => {
	it('flips visible from false to true', () => {
		chat.visible = false;
		toggleChat();
		expect(chat.visible).toBe(true);
	});

	it('flips visible from true to false', () => {
		chat.visible = true;
		toggleChat();
		expect(chat.visible).toBe(false);
	});

	it('round-trips back to original after two calls', () => {
		const original = chat.visible;
		toggleChat();
		toggleChat();
		expect(chat.visible).toBe(original);
	});
});

describe('recentMessages', () => {
	function fillMessages(n: number) {
		const msgs: ChatMessage[] = [];
		for (let i = 0; i < n; i++) {
			msgs.push({ role: 'user', text: `msg-${i}`, timestamp: i });
		}
		chat.messages = msgs;
	}

	it('returns [] when there are no messages', () => {
		expect(recentMessages()).toEqual([]);
	});

	it('returns all messages when count is below the 200 cap', () => {
		fillMessages(50);
		expect(recentMessages()).toHaveLength(50);
	});

	it('returns exactly 200 when count equals the cap', () => {
		fillMessages(200);
		expect(recentMessages()).toHaveLength(200);
	});

	it('returns the most recent 200 when count exceeds the cap', () => {
		fillMessages(250);
		const recent = recentMessages();
		expect(recent).toHaveLength(200);
		// First returned message should be msg-50 (250 - 200)
		expect(recent[0].text).toBe('msg-50');
		expect(recent[recent.length - 1].text).toBe('msg-249');
	});
});
