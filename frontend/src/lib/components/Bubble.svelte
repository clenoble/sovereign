<script lang="ts">
	import { bubbleState } from '$lib/stores/app';
	import { chat } from '$lib/stores/chat';

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
	class:animating={isAnimating($bubbleState)}
	style="--border-color: {borderColor($bubbleState)}"
	onclick={() => chat.toggle()}
	title="Chat with AI"
>
	<svg width="48" height="48" viewBox="0 0 32 32" fill="none">
		<circle cx="16" cy="16" r="14" fill="url(#bubbleGrad)" stroke={borderColor($bubbleState)} stroke-width="1.5" />
		<g transform="translate(16, 16)">
			<path d="M -5 2 L -6 4 L 6 4 L 5 2 Z" fill="url(#crownGrad)" />
			<path d="M -5 2 L -4 -4 L -3 2 M -1 2 L 0 -5 L 1 2 M 3 2 L 4 -4 L 5 2" fill="url(#crownGrad)" />
		</g>
		{#if isAnimating($bubbleState)}
			<circle cx="16" cy="16" r="14" fill="none" stroke={borderColor($bubbleState)} stroke-width="2" opacity="0.3">
				<animate attributeName="r" values="14;18;14" dur="2s" repeatCount="indefinite" />
				<animate attributeName="opacity" values="0.3;0;0.3" dur="2s" repeatCount="indefinite" />
			</circle>
		{/if}
		<defs>
			<radialGradient id="bubbleGrad">
				<stop offset="0%" stop-color="#FCD34D" />
				<stop offset="50%" stop-color="#F59E0B" />
				<stop offset="100%" stop-color="#D97706" />
			</radialGradient>
			<linearGradient id="crownGrad" x1="0%" y1="0%" x2="0%" y2="100%">
				<stop offset="0%" stop-color="#CD7F32" />
				<stop offset="50%" stop-color="#A0522D" />
				<stop offset="100%" stop-color="#8B4513" />
			</linearGradient>
		</defs>
	</svg>
</button>

<style>
	.bubble {
		position: fixed;
		bottom: 70px;
		left: 50%;
		transform: translateX(-50%);
		background: none;
		border: none;
		cursor: pointer;
		z-index: 90;
		padding: 0;
		transition: transform 0.2s ease;
	}

	.bubble:hover {
		transform: translateX(-50%) scale(1.1);
	}

	.bubble.animating svg {
		filter: drop-shadow(0 0 8px var(--border-color));
	}
</style>
