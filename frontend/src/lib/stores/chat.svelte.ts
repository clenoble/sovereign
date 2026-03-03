/** Rune-based reactive state for the chat panel. */

export interface ChatMessage {
	role: 'user' | 'assistant' | 'system';
	text: string;
	timestamp: number;
}

/** Reactive chat state. */
export const chat = $state({
	messages: [] as ChatMessage[],
	generating: false,
	input: '',
	visible: false
});

export function pushUser(text: string) {
	chat.messages = [
		...chat.messages,
		{ role: 'user' as const, text, timestamp: Date.now() }
	];
	chat.generating = true;
}

export function pushAssistant(text: string) {
	chat.messages = [
		...chat.messages,
		{ role: 'assistant' as const, text, timestamp: Date.now() }
	];
	chat.generating = false;
}

export function pushSystem(text: string) {
	chat.messages = [
		...chat.messages,
		{ role: 'system' as const, text, timestamp: Date.now() }
	];
}

export function clearGenerating() {
	chat.generating = false;
}

export function toggleChat() {
	chat.visible = !chat.visible;
}

/** Recent messages for display (last 200). */
export function recentMessages(): ChatMessage[] {
	return chat.messages.slice(-200);
}
