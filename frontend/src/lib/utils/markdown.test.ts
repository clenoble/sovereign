// @vitest-environment jsdom
//
// jsdom, not the project-default happy-dom: DOMPurify mis-sanitizes under
// happy-dom (it left a javascript: href intact in this suite), so these
// security assertions need a DOM faithful to real browsers.
import { describe, it, expect } from 'vitest';
import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
	it('renders basic markdown', () => {
		const html = renderMarkdown('**bold** and *italic*');
		expect(html).toContain('<strong>bold</strong>');
		expect(html).toContain('<em>italic</em>');
	});

	it('escapes raw HTML instead of passing it through', () => {
		const html = renderMarkdown('<img src=x onerror=alert(1)>');
		expect(html).not.toContain('<img');
		expect(html).toContain('escaped-html');
	});

	it('strips javascript: URLs', () => {
		const html = renderMarkdown('[click](javascript:alert(1))');
		expect(html).not.toContain('javascript:');
	});

	it('forces external links into a new window without opener', () => {
		const html = renderMarkdown('[site](https://example.com)');
		expect(html).toContain('target="_blank"');
		expect(html).toContain('rel="noopener noreferrer"');
	});

	it('leaves relative/anchor links without target', () => {
		const html = renderMarkdown('[head](#section)');
		expect(html).toContain('href="#section"');
		expect(html).not.toContain('target="_blank"');
	});
});

describe('renderMarkdown — DOS-001 hardening', () => {
	it('truncates input beyond the size cap and notes it', () => {
		const huge = 'a'.repeat(300 * 1024); // > 256 KB cap
		const out = renderMarkdown(huge);
		expect(out).toContain('content truncated');
		expect(out.length).toBeLessThan(huge.length);
	});

	it('does not throw on pathological deeply-nested input', () => {
		// Deeply-nested blockquotes overflow marked's parser stack; the
		// try/catch must degrade to escaped plaintext rather than throw inside
		// the Svelte $derived (which would break the whole reactive render).
		const evil = '> '.repeat(20000); // ~40 KB, deep nesting, under the cap
		expect(() => renderMarkdown(evil)).not.toThrow();
	});

	it('handles empty and nullish input', () => {
		expect(() => renderMarkdown('')).not.toThrow();
		// @ts-expect-error exercising the null guard at the JS boundary
		expect(() => renderMarkdown(undefined)).not.toThrow();
	});
});
