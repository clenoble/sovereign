<script lang="ts">
	import { onMount } from 'svelte';
	import { getStatus } from '$lib/api/commands';
	import { orchestratorAvailable } from '$lib/stores/app';

	let status = $state<{
		documents: number;
		threads: number;
		contacts: number;
		orchestrator_available: boolean;
	} | null>(null);
	let error = $state('');

	onMount(async () => {
		try {
			status = await getStatus();
			orchestratorAvailable.set(status.orchestrator_available);
		} catch (e) {
			error = String(e);
		}
	});
</script>

<main>
	<div class="canvas-area">
		<!-- Phase 3: HTML5 Canvas renders here -->
		<div class="center-card">
			<svg width="56" height="56" viewBox="0 0 32 32" fill="none">
				<circle cx="16" cy="16" r="14" fill="url(#coreGrad)" />
				<g transform="translate(16, 16)">
					<path d="M -5 2 L -6 4 L 6 4 L 5 2 Z" fill="url(#crownGrad)" />
					<path
						d="M -5 2 L -4 -4 L -3 2 M -1 2 L 0 -5 L 1 2 M 3 2 L 4 -4 L 5 2"
						fill="url(#crownGrad)"
					/>
				</g>
				<defs>
					<radialGradient id="coreGrad">
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

			<h1>Sovereign GE</h1>

			{#if status}
				<div class="stats">
					<span>{status.documents} documents</span>
					<span class="sep">&middot;</span>
					<span>{status.threads} threads</span>
					<span class="sep">&middot;</span>
					<span>{status.contacts} contacts</span>
					<span class="sep">&middot;</span>
					<span class:active={status.orchestrator_available}>
						AI {status.orchestrator_available ? 'online' : 'offline'}
					</span>
				</div>
			{/if}

			{#if error}
				<p class="error">{error}</p>
			{/if}

			<p class="hint">Phase 1 — Chat, Search, Taskbar</p>
		</div>
	</div>
</main>

<style>
	main {
		flex: 1;
		display: flex;
		padding-bottom: 44px; /* taskbar height */
	}

	.canvas-area {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		position: relative;
	}

	.center-card {
		text-align: center;
	}

	h1 {
		font-size: 2rem;
		font-weight: 300;
		margin: 1rem 0 0.5rem;
		color: var(--accent);
	}

	.stats {
		display: flex;
		gap: 0.5rem;
		justify-content: center;
		color: var(--text-secondary);
		font-size: 0.85rem;
		margin-top: 1rem;
	}

	.sep {
		color: var(--text-muted);
	}

	.active {
		color: var(--success);
	}

	.error {
		color: var(--error);
		font-size: 0.85rem;
	}

	.hint {
		margin-top: 2rem;
		color: var(--text-muted);
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}
</style>
