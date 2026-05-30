/** Browser panel state — Svelte 5 rune store. */

import type { ReliabilityResultDto } from '$lib/api/commands';

export const browser = $state({
	isOpen: false,
	url: '',
	title: '',
	loading: false,
	/** Extracted page text (for reader mode + assessment) */
	extractedText: '',
	/** Reliability assessment result */
	reliability: null as ReliabilityResultDto | null,
	assessing: false,
	/** Panel bounds synced with Tauri webview */
	bounds: { x: 0, y: 0, width: 0, height: 0 }
});

/**
 * Set browser panel state to open. This only updates UI state —
 * the actual Tauri webview is created by BrowserPanel on mount
 * (which reads `browser.bounds` to position the webview).
 */
export function openBrowser(url: string) {
	browser.url = url;
	browser.isOpen = true;
	browser.loading = true;
	browser.extractedText = '';
	browser.reliability = null;
}

export function closeBrowser() {
	browser.isOpen = false;
	browser.url = '';
	browser.title = '';
	browser.extractedText = '';
	browser.reliability = null;
	browser.loading = false;
}

export function setBrowserNavigated(url: string, title: string) {
	browser.url = url;
	browser.title = title;
	browser.loading = false;
	// Clear stale assessment when navigating
	browser.extractedText = '';
	browser.reliability = null;
}

export function setBrowserContentExtracted(text: string) {
	browser.extractedText = text;
}

export function setBrowserReliability(result: ReliabilityResultDto) {
	browser.reliability = result;
	browser.assessing = false;
}

export function setBrowserAssessing(assessing: boolean) {
	browser.assessing = assessing;
}

export function updateBrowserBounds(bounds: { x: number; y: number; width: number; height: number }) {
	browser.bounds = bounds;
}
