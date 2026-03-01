import { writable, derived } from 'svelte/store';

export interface ChatMessage {
	role: 'user' | 'assistant' | 'system';
	text: string;
	timestamp: number;
}

function createChatStore() {
	const messages = writable<ChatMessage[]>([]);
	const generating = writable(false);
	const input = writable('');
	const visible = writable(false);

	return {
		messages,
		generating,
		input,
		visible,

		pushUser(text: string) {
			messages.update((m) => [
				...m,
				{ role: 'user', text, timestamp: Date.now() }
			]);
			generating.set(true);
		},

		pushAssistant(text: string) {
			messages.update((m) => [
				...m,
				{ role: 'assistant', text, timestamp: Date.now() }
			]);
			generating.set(false);
		},

		pushSystem(text: string) {
			messages.update((m) => [
				...m,
				{ role: 'system', text, timestamp: Date.now() }
			]);
		},

		clearGenerating() {
			generating.set(false);
		},

		toggle() {
			visible.update((v) => !v);
		},

		/** Recent messages for display (last 200). */
		recent: derived(messages, ($m) => $m.slice(-200))
	};
}

export const chat = createChatStore();
