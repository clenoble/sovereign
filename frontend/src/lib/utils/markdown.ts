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
			return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
		}
	}
});

export function renderMarkdown(text: string): string {
	return DOMPurify.sanitize(marked.parse(text) as string);
}
