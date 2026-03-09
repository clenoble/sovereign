<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { toggleChat } from '$lib/stores/chat.svelte';
	import { suggestions, toggleSuggestions } from '$lib/stores/suggestions.svelte';
	import BubblePreview from './BubblePreview.svelte';

	// Map bubble state to border color
	function borderColor(state: string): string {
		switch (state) {
			case 'ProcessingOwned':
			case 'ProcessingExternal':
				return 'var(--bubble-processing)';
			case 'Executing':
				return 'var(--bubble-executing)';
			case 'Proposing':
				return 'var(--bubble-proposing)';
			case 'Suggesting':
				return 'var(--bubble-suggesting)';
			default:
				return 'var(--bubble-idle)';
		}
	}

	function isAnimating(state: string): boolean {
		return state !== 'Idle';
	}
</script>

<button
	class="bubble"
	class:animating={isAnimating(app.bubbleState)}
	style="--border-color: {borderColor(app.bubbleState)}"
	onclick={() => toggleChat()}
	title="Chat with AI"
>
	<BubblePreview style={app.bubbleStyle} size={96} />
	{#if isAnimating(app.bubbleState)}
		<svg class="state-ring" width="96" height="96" viewBox="0 0 120 120">
			<circle cx="60" cy="60" r="56" fill="none" stroke={borderColor(app.bubbleState)} stroke-width="4" opacity="0.3">
				<animate attributeName="r" values="56;64;56" dur="2s" repeatCount="indefinite" />
				<animate attributeName="opacity" values="0.3;0;0.3" dur="2s" repeatCount="indefinite" />
			</circle>
		</svg>
	{/if}
	{#if suggestions.pending.length > 0}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<span class="suggestion-badge" onclick={(e) => { e.stopPropagation(); toggleSuggestions(); }}>
			{suggestions.pending.length}
		</span>
	{/if}
</button>

<style>
	.bubble {
		position: fixed;
		top: 16px;
		left: 16px;
		right: auto;
		background: none;
		border: none;
		cursor: pointer;
		z-index: 90;
		padding: 0;
		transition: transform 0.2s ease;
	}

	.bubble:hover {
		transform: scale(1.1);
	}

	.bubble.animating {
		filter: drop-shadow(0 0 8px var(--border-color));
	}

	.state-ring {
		position: absolute;
		top: 0;
		left: 0;
		pointer-events: none;
	}

	.suggestion-badge {
		position: absolute;
		top: -2px;
		right: -2px;
		min-width: 20px;
		height: 20px;
		border-radius: 10px;
		background: var(--accent, #6366f1);
		color: #fff;
		font-size: 0.7rem;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 5px;
		cursor: pointer;
		pointer-events: auto;
	}
</style>
