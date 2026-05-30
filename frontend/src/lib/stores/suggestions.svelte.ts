/** AI-suggested link store — Svelte 5 rune store. */

export interface LinkSuggestion {
	id: string;
	fromDocId: string;
	fromTitle: string;
	toDocId: string;
	toTitle: string;
	relationType: string;
	strength: number;
	rationale: string;
	source: string;
}

export const suggestions = $state({
	pending: [] as LinkSuggestion[],
	visible: false
});

export function addSuggestion(s: LinkSuggestion) {
	// Avoid duplicates
	if (!suggestions.pending.some((p) => p.id === s.id)) {
		suggestions.pending.push(s);
	}
}

export function removeSuggestion(id: string) {
	suggestions.pending = suggestions.pending.filter((s) => s.id !== id);
}

export function setSuggestions(list: LinkSuggestion[]) {
	suggestions.pending = list;
}

export function toggleSuggestions() {
	suggestions.visible = !suggestions.visible;
}
