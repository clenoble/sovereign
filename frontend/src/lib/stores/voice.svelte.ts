/** Voice pipeline state store — Svelte 5 rune store.
 *
 * Driven by the backend `voice-event` Tauri emit (see events.ts). Reflects
 * the listening / transcribing / speaking state of the Jiminy voice pipeline
 * so the mic button (Taskbar / Bubble) can show live feedback.
 */

import { pushUser } from './chat.svelte';

export const voice = $state({
	listening: false,
	transcribing: false,
	speaking: false,
	lastTranscript: ''
});

let speakingTimer: ReturnType<typeof setTimeout> | null = null;

function clearSpeakingTimer() {
	if (speakingTimer !== null) {
		clearTimeout(speakingTimer);
		speakingTimer = null;
	}
}

/** Light the "speaking" indicator while the assistant's reply is delivered.
 *
 * The robot speaks each `ChatResponse` through the Jiminy sidecar's Piper TTS,
 * which sends no completion signal back, so we auto-clear after an estimate
 * proportional to the reply length. Any subsequent backend `voice-event`
 * (listening / transcription / idle) cancels the estimate. */
export function markSpeaking(text: string) {
	clearSpeakingTimer();
	voice.listening = false;
	voice.transcribing = false;
	voice.speaking = true;
	const ms = Math.min(15000, Math.max(1500, text.length * 55));
	speakingTimer = setTimeout(() => {
		voice.speaking = false;
		speakingTimer = null;
	}, ms);
}

/** Apply a backend `voice-event` payload onto the store flags. */
export function applyVoiceEvent(kind: string, text?: string) {
	clearSpeakingTimer();
	switch (kind) {
		case 'listening':
			voice.listening = true;
			voice.transcribing = false;
			voice.speaking = false;
			break;
		case 'transcription':
			voice.listening = false;
			voice.transcribing = false;
			voice.speaking = false;
			if (text) {
				voice.lastTranscript = text;
				pushUser(text); // surface the spoken text in the chat window
			}
			break;
		case 'speaking':
			voice.listening = false;
			voice.transcribing = false;
			voice.speaking = true;
			break;
		case 'idle':
		default:
			voice.listening = false;
			voice.transcribing = false;
			voice.speaking = false;
			break;
	}
}
