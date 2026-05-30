/** Voice pipeline state store — Svelte 5 rune store.
 *
 * Driven by the backend `voice-event` Tauri emit (see events.ts). Reflects
 * the listening / transcribing / speaking state of the Jiminy voice pipeline
 * so the mic button (Taskbar / Bubble) can show live feedback — the Tauri
 * replacement for the retired Iced taskbar's voice indicator.
 */

export const voice = $state({
	listening: false,
	transcribing: false,
	speaking: false,
	lastTranscript: ''
});

/** Apply a backend `voice-event` payload onto the store flags. */
export function applyVoiceEvent(kind: string, text?: string) {
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
			if (text) voice.lastTranscript = text;
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
