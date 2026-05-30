import { beforeEach, describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import Bubble from './Bubble.svelte';
import { app, type BubbleState } from '$lib/stores/app.svelte';
import { chat } from '$lib/stores/chat.svelte';
import { suggestions, type LinkSuggestion } from '$lib/stores/suggestions.svelte';

function makeSuggestion(id: string): LinkSuggestion {
	return {
		id,
		fromDocId: 'd:1',
		fromTitle: 'A',
		toDocId: 'd:2',
		toTitle: 'B',
		relationType: 'supports',
		strength: 0.8,
		rationale: 'because',
		source: 'consolidation'
	};
}

beforeEach(() => {
	app.bubbleState = 'Idle';
	app.bubbleStyle = 'icon';
	chat.visible = false;
	suggestions.pending = [];
	suggestions.visible = false;
});

describe('Bubble — base rendering', () => {
	it('renders a button with a "Chat with AI" tooltip', () => {
		const { getByTitle } = render(Bubble);
		expect(getByTitle('Chat with AI')).toBeInTheDocument();
	});

	it('renders the BubblePreview SVG inside the button', () => {
		const { container } = render(Bubble);
		const svg = container.querySelector('button.bubble svg');
		expect(svg).not.toBeNull();
	});
});

describe('Bubble — bubbleState → border color mapping', () => {
	const cases: Array<[BubbleState, string]> = [
		['Idle', 'var(--bubble-idle)'],
		['ProcessingOwned', 'var(--bubble-processing)'],
		['ProcessingExternal', 'var(--bubble-processing)'],
		['Executing', 'var(--bubble-executing)'],
		['Proposing', 'var(--bubble-proposing)'],
		['Suggesting', 'var(--bubble-suggesting)']
	];

	for (const [state, color] of cases) {
		it(`state "${state}" sets --border-color to "${color}"`, () => {
			app.bubbleState = state;
			const { container } = render(Bubble);
			const button = container.querySelector('button.bubble') as HTMLElement;
			expect(button.style.getPropertyValue('--border-color')).toBe(color);
		});
	}
});

describe('Bubble — animating state', () => {
	it('Idle → no .animating class and no state-ring rendered', () => {
		app.bubbleState = 'Idle';
		const { container } = render(Bubble);
		const button = container.querySelector('button.bubble') as HTMLElement;
		expect(button.classList.contains('animating')).toBe(false);
		expect(container.querySelector('.state-ring')).toBeNull();
	});

	const animatingStates: BubbleState[] = [
		'ProcessingOwned',
		'ProcessingExternal',
		'Executing',
		'Proposing',
		'Suggesting'
	];

	for (const state of animatingStates) {
		it(`${state} → .animating class is present and .state-ring is rendered`, () => {
			app.bubbleState = state;
			const { container } = render(Bubble);
			const button = container.querySelector('button.bubble') as HTMLElement;
			expect(button.classList.contains('animating')).toBe(true);
			expect(container.querySelector('.state-ring')).not.toBeNull();
		});
	}

	it('state-ring stroke matches the border color of the current state', () => {
		app.bubbleState = 'Suggesting';
		const { container } = render(Bubble);
		const ringCircle = container.querySelector('.state-ring circle');
		expect(ringCircle?.getAttribute('stroke')).toBe('var(--bubble-suggesting)');
	});

	it('reactively renders the state-ring when bubbleState changes from Idle to an animating state', async () => {
		app.bubbleState = 'Idle';
		const { container } = render(Bubble);
		expect(container.querySelector('.state-ring')).toBeNull();

		app.bubbleState = 'Executing';
		await tick();

		expect(container.querySelector('.state-ring')).not.toBeNull();
	});

	it('reactively removes the state-ring when bubbleState changes back to Idle', async () => {
		app.bubbleState = 'Proposing';
		const { container } = render(Bubble);
		expect(container.querySelector('.state-ring')).not.toBeNull();

		app.bubbleState = 'Idle';
		await tick();

		expect(container.querySelector('.state-ring')).toBeNull();
	});
});

describe('Bubble — click toggles chat', () => {
	it('clicking the bubble flips chat.visible', async () => {
		chat.visible = false;
		const { getByTitle } = render(Bubble);
		const btn = getByTitle('Chat with AI') as HTMLButtonElement;

		btn.click();
		expect(chat.visible).toBe(true);

		btn.click();
		expect(chat.visible).toBe(false);
	});
});

describe('Bubble — suggestion badge', () => {
	it('does NOT render the badge when suggestions.pending is empty', () => {
		suggestions.pending = [];
		const { container } = render(Bubble);
		expect(container.querySelector('.suggestion-badge')).toBeNull();
	});

	it('renders the badge with the correct count when suggestions are present', () => {
		suggestions.pending = [
			makeSuggestion('s:1'),
			makeSuggestion('s:2'),
			makeSuggestion('s:3')
		];
		const { container } = render(Bubble);
		const badge = container.querySelector('.suggestion-badge');
		expect(badge).not.toBeNull();
		expect(badge?.textContent?.trim()).toBe('3');
	});

	it('badge appears reactively when a suggestion is added', async () => {
		suggestions.pending = [];
		const { container } = render(Bubble);
		expect(container.querySelector('.suggestion-badge')).toBeNull();

		suggestions.pending = [makeSuggestion('s:1')];
		await tick();

		expect(container.querySelector('.suggestion-badge')).not.toBeNull();
	});

	it('badge count updates reactively when suggestions are added', async () => {
		suggestions.pending = [makeSuggestion('s:1')];
		const { container } = render(Bubble);
		expect(container.querySelector('.suggestion-badge')?.textContent?.trim()).toBe('1');

		suggestions.pending = [
			makeSuggestion('s:1'),
			makeSuggestion('s:2'),
			makeSuggestion('s:3'),
			makeSuggestion('s:4'),
			makeSuggestion('s:5')
		];
		await tick();

		expect(container.querySelector('.suggestion-badge')?.textContent?.trim()).toBe('5');
	});

	it('badge disappears reactively when suggestions become empty', async () => {
		suggestions.pending = [makeSuggestion('s:1')];
		const { container } = render(Bubble);
		expect(container.querySelector('.suggestion-badge')).not.toBeNull();

		suggestions.pending = [];
		await tick();

		expect(container.querySelector('.suggestion-badge')).toBeNull();
	});

	it('clicking the badge toggles suggestions panel and STOPS PROPAGATION (chat.visible unchanged)', () => {
		suggestions.pending = [makeSuggestion('s:1')];
		chat.visible = false;
		suggestions.visible = false;

		const { container } = render(Bubble);
		const badge = container.querySelector('.suggestion-badge') as HTMLElement;
		expect(badge).not.toBeNull();

		badge.click();

		expect(suggestions.visible).toBe(true);
		expect(chat.visible).toBe(false);
	});

	it('clicking the badge a second time toggles suggestions back off', () => {
		suggestions.pending = [makeSuggestion('s:1')];
		suggestions.visible = false;

		const { container } = render(Bubble);
		const badge = container.querySelector('.suggestion-badge') as HTMLElement;

		badge.click();
		expect(suggestions.visible).toBe(true);

		badge.click();
		expect(suggestions.visible).toBe(false);
	});
});

describe('Bubble — bubbleStyle prop wiring (BubblePreview)', () => {
	it('passes app.bubbleStyle="icon" to BubblePreview (renders icon-style SVG)', () => {
		app.bubbleStyle = 'icon';
		const { container } = render(Bubble);
		// Each BubblePreview style has a unique gradient / def id we can pin on
		expect(container.querySelector('#iconCoreGradient')).not.toBeNull();
	});

	it('renders the wave style when app.bubbleStyle = "wave"', () => {
		app.bubbleStyle = 'wave';
		const { container } = render(Bubble);
		expect(container.querySelector('#waveGradient')).not.toBeNull();
		expect(container.querySelector('#iconCoreGradient')).toBeNull();
	});

	it('reactively swaps the BubblePreview SVG when bubbleStyle changes', async () => {
		app.bubbleStyle = 'icon';
		const { container } = render(Bubble);
		expect(container.querySelector('#iconCoreGradient')).not.toBeNull();

		app.bubbleStyle = 'pulse';
		await tick();

		expect(container.querySelector('#iconCoreGradient')).toBeNull();
		expect(container.querySelector('#pinkGradient')).not.toBeNull();
	});

	it('renders all known bubble styles without errors', () => {
		const styles = ['icon', 'wave', 'spin', 'pulse', 'blink', 'rings', 'matrix', 'orbit', 'morph'];
		for (const style of styles) {
			app.bubbleStyle = style;
			const { container, unmount } = render(Bubble);
			// Smoke check: button + svg present
			expect(container.querySelector('button.bubble')).not.toBeNull();
			expect(container.querySelector('button.bubble svg')).not.toBeNull();
			unmount();
		}
	});
});

describe('Bubble — drop-shadow filter via animating class (visual regression guard)', () => {
	it('animating class enables the drop-shadow filter (per CSS rules in Bubble.svelte)', () => {
		// We can't easily assert computed style across happy-dom for filter,
		// but we can guard against a regression where the .animating class
		// stops being applied. The CSS rule .bubble.animating { filter: ... }
		// is what makes the bubble glow during state changes.
		app.bubbleState = 'Executing';
		const { container } = render(Bubble);
		const button = container.querySelector('button.bubble') as HTMLElement;
		expect(button.classList.contains('animating')).toBe(true);
	});
});
