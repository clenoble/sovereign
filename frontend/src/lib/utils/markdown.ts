import { marked } from 'marked';
import DOMPurify from 'dompurify';

marked.setOptions({ breaks: true, gfm: true });

export function renderMarkdown(text: string): string {
	return DOMPurify.sanitize(marked.parse(text) as string);
}
