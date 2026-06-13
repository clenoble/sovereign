import { marked } from 'marked';
import DOMPurify from 'dompurify';

marked.use({
	breaks: true,
	gfm: true,
	renderer: {
		// Escape raw HTML in markdown source instead of passing it through.
		// Markdown formatting (bold, code, lists, etc.) still works normally
		// since those go through their own renderer methods.
		html({ text }: { text: string }) {
			const escaped = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
			return `<span class="escaped-html">// ${escaped.trim()}</span>`;
		}
	}
});

// AI- and document-sourced markdown can contain links to arbitrary origins.
// Without this hook a click navigates the MAIN app webview (which holds the
// IPC bridge) off-app — a phishing/UI-spoof vector. Force external links into
// a new context and sever the opener relationship.
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
	if (node.tagName === 'A' && node.hasAttribute('href')) {
		const href = node.getAttribute('href') ?? '';
		if (/^https?:\/\//i.test(href)) {
			node.setAttribute('target', '_blank');
			node.setAttribute('rel', 'noopener noreferrer');
		}
	}
});

// DOS-001: cap input before the synchronous marked + DOMPurify parse on the
// main thread. Document bodies and chat text are attacker-influenceable
// (P2P-synced rows, imported email/Signal, saved web pages, pasted content);
// a few-thousand-level nested structure throws "Maximum call stack size
// exceeded" and multi-MB input drives the WebView to a heap-OOM — either way
// killing the sole UI just by opening a synced/imported doc or an AI echo.
const MAX_MARKDOWN_CHARS = 256 * 1024; // ~256 KB

function escapeHtml(s: string): string {
	return s
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;');
}

export function renderMarkdown(text: string): string {
	let input = text ?? '';
	let truncated = false;
	if (input.length > MAX_MARKDOWN_CHARS) {
		input = input.slice(0, MAX_MARKDOWN_CHARS);
		truncated = true;
	}

	let html: string;
	try {
		html = marked.parse(input) as string;
	} catch {
		// Pathological markdown (e.g. deeply nested blockquotes) overflows the
		// parser stack. Degrade to safe escaped plaintext rather than throw
		// inside a Svelte `$derived`, which would break the whole reactive render.
		html = `<pre class="markdown-fallback">${escapeHtml(input)}</pre>`;
	}

	const clean = DOMPurify.sanitize(html);
	// The note is trusted static markup appended AFTER sanitization.
	return truncated
		? `${clean}<p class="markdown-truncated"><em>… content truncated for display</em></p>`
		: clean;
}
