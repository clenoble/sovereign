import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { applyVoiceEvent, markSpeaking, voice } from './voice.svelte';

beforeEach(() => {
	voice.listening = false;
	voice.transcribing = false;
	voice.speaking = false;
	voice.lastTranscript = '';
	vi.useRealTimers();
});

afterEach(() => {
	vi.useRealTimers();
});

describe('applyVoiceEvent', () => {
	it('listening sets listening and clears the other flags', () => {
		voice.speaking = true;
		applyVoiceEvent('listening');
		expect(voice.listening).toBe(true);
		expect(voice.speaking).toBe(false);
		expect(voice.transcribing).toBe(false);
	});

	it('transcription stores the text and clears flags', () => {
		applyVoiceEvent('transcription', 'open the planning doc');
		expect(voice.lastTranscript).toBe('open the planning doc');
		expect(voice.listening).toBe(false);
		expect(voice.transcribing).toBe(false);
		expect(voice.speaking).toBe(false);
	});

	it('speaking sets the speaking flag', () => {
		applyVoiceEvent('speaking');
		expect(voice.speaking).toBe(true);
	});

	it('idle and unknown kinds clear every flag', () => {
		voice.listening = true;
		applyVoiceEvent('idle');
		expect(voice.listening).toBe(false);
		expect(voice.speaking).toBe(false);
		voice.speaking = true;
		applyVoiceEvent('something-unexpected');
		expect(voice.speaking).toBe(false);
	});
});

describe('markSpeaking', () => {
	it('lights the speaking indicator immediately', () => {
		vi.useFakeTimers();
		markSpeaking('hello there');
		expect(voice.speaking).toBe(true);
	});

	it('auto-clears speaking after the length-based window (no TtsDone arrives)', () => {
		vi.useFakeTimers();
		markSpeaking('hi'); // short text -> minimum 1500ms window
		expect(voice.speaking).toBe(true);
		vi.advanceTimersByTime(1600);
		expect(voice.speaking).toBe(false);
	});

	it('a backend voice-event cancels the pending auto-clear', () => {
		vi.useFakeTimers();
		markSpeaking('a longer reply that would otherwise stay lit for a while');
		expect(voice.speaking).toBe(true);
		applyVoiceEvent('listening'); // must clear both the timer and speaking
		expect(voice.speaking).toBe(false);
		expect(voice.listening).toBe(true);
		vi.advanceTimersByTime(20000); // the cancelled timer must not re-fire
		expect(voice.listening).toBe(true);
		expect(voice.speaking).toBe(false);
	});

	it('re-marking resets the window (first timer does not clear early)', () => {
		vi.useFakeTimers();
		markSpeaking('first');
		vi.advanceTimersByTime(1000);
		markSpeaking('second message is a fair bit longer than the first one');
		vi.advanceTimersByTime(1000); // still within the new window
		expect(voice.speaking).toBe(true);
	});
});
