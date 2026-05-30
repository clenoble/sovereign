import { beforeEach, describe, expect, it } from 'vitest';
import type { ReliabilityResultDto } from '$lib/api/commands';
import {
	browser,
	closeBrowser,
	openBrowser,
	setBrowserAssessing,
	setBrowserContentExtracted,
	setBrowserNavigated,
	setBrowserReliability,
	updateBrowserBounds
} from './browser.svelte';

function makeReliability(): ReliabilityResultDto {
	return {
		classification: 'Factual',
		final_score: 4.2,
		raw_assessment: [
			{ indicator: 'sourcing', analysis: 'cites primary sources', score: 5 }
		]
	};
}

beforeEach(() => {
	browser.isOpen = false;
	browser.url = '';
	browser.title = '';
	browser.loading = false;
	browser.extractedText = '';
	browser.reliability = null;
	browser.assessing = false;
	browser.bounds = { x: 0, y: 0, width: 0, height: 0 };
});

describe('openBrowser', () => {
	it('sets url and flips isOpen + loading to true', () => {
		openBrowser('https://example.com');
		expect(browser.url).toBe('https://example.com');
		expect(browser.isOpen).toBe(true);
		expect(browser.loading).toBe(true);
	});

	it('clears any stale extracted text and reliability from a previous session', () => {
		browser.extractedText = 'old page text';
		browser.reliability = makeReliability();

		openBrowser('https://new.example.com');

		expect(browser.extractedText).toBe('');
		expect(browser.reliability).toBeNull();
	});

	it('does not touch bounds (those are managed by the panel component)', () => {
		browser.bounds = { x: 10, y: 20, width: 800, height: 600 };
		openBrowser('https://example.com');
		expect(browser.bounds).toEqual({ x: 10, y: 20, width: 800, height: 600 });
	});
});

describe('closeBrowser', () => {
	it('resets all content fields and flags', () => {
		browser.isOpen = true;
		browser.url = 'https://example.com';
		browser.title = 'Example';
		browser.loading = true;
		browser.extractedText = 'page text';
		browser.reliability = makeReliability();

		closeBrowser();

		expect(browser.isOpen).toBe(false);
		expect(browser.url).toBe('');
		expect(browser.title).toBe('');
		expect(browser.loading).toBe(false);
		expect(browser.extractedText).toBe('');
		expect(browser.reliability).toBeNull();
	});

	it('does not touch assessing flag (caller controls that)', () => {
		browser.assessing = true;
		closeBrowser();
		expect(browser.assessing).toBe(true);
	});
});

describe('setBrowserNavigated', () => {
	it('updates url and title', () => {
		browser.isOpen = true;
		setBrowserNavigated('https://example.com/page', 'Page Title');
		expect(browser.url).toBe('https://example.com/page');
		expect(browser.title).toBe('Page Title');
	});

	it('flips loading off (the navigation completed)', () => {
		browser.loading = true;
		setBrowserNavigated('https://example.com', 'X');
		expect(browser.loading).toBe(false);
	});

	it('clears stale assessment so the panel never shows reliability for a different URL', () => {
		browser.extractedText = 'previous page text';
		browser.reliability = makeReliability();

		setBrowserNavigated('https://new.example.com', 'New');

		expect(browser.extractedText).toBe('');
		expect(browser.reliability).toBeNull();
	});
});

describe('setBrowserContentExtracted', () => {
	it('stores the extracted text', () => {
		setBrowserContentExtracted('extracted body content');
		expect(browser.extractedText).toBe('extracted body content');
	});

	it('does not touch reliability or assessing', () => {
		browser.reliability = makeReliability();
		browser.assessing = true;
		setBrowserContentExtracted('new text');
		expect(browser.reliability).not.toBeNull();
		expect(browser.assessing).toBe(true);
	});
});

describe('setBrowserReliability', () => {
	it('stores the result and flips assessing off', () => {
		browser.assessing = true;
		const result = makeReliability();
		setBrowserReliability(result);
		// Use deep equality — Svelte 5 $state wraps the value in a Proxy so
		// identity comparison (`toBe`) is not appropriate.
		expect(browser.reliability).toStrictEqual(result);
		expect(browser.assessing).toBe(false);
	});
});

describe('setBrowserAssessing', () => {
	it('toggles assessing flag', () => {
		setBrowserAssessing(true);
		expect(browser.assessing).toBe(true);
		setBrowserAssessing(false);
		expect(browser.assessing).toBe(false);
	});
});

describe('updateBrowserBounds', () => {
	it('replaces the bounds object', () => {
		updateBrowserBounds({ x: 100, y: 50, width: 1024, height: 768 });
		expect(browser.bounds).toEqual({ x: 100, y: 50, width: 1024, height: 768 });
	});
});

describe('navigation lifecycle (integration)', () => {
	it('open → extract → assess → navigate-away clears stale data correctly', () => {
		openBrowser('https://a.example.com');
		expect(browser.url).toBe('https://a.example.com');

		setBrowserContentExtracted('content of A');
		setBrowserAssessing(true);
		setBrowserReliability(makeReliability());
		expect(browser.assessing).toBe(false);
		expect(browser.reliability).not.toBeNull();

		// User navigates within the embedded webview — stale assessment must clear
		setBrowserNavigated('https://b.example.com', 'B');
		expect(browser.url).toBe('https://b.example.com');
		expect(browser.extractedText).toBe('');
		expect(browser.reliability).toBeNull();
	});

	it('close while assessing leaves no orphaned state on next open', () => {
		openBrowser('https://a.example.com');
		setBrowserAssessing(true);
		closeBrowser();

		openBrowser('https://b.example.com');
		expect(browser.url).toBe('https://b.example.com');
		expect(browser.reliability).toBeNull();
		expect(browser.extractedText).toBe('');
		expect(browser.loading).toBe(true);
	});
});
