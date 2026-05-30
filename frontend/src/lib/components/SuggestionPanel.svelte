<script lang="ts">
	import { suggestions, removeSuggestion, toggleSuggestions } from '$lib/stores/suggestions.svelte';
	import { acceptLinkSuggestion, dismissLinkSuggestion } from '$lib/api/commands';

	async function accept(id: string) {
		try {
			await acceptLinkSuggestion(id);
			removeSuggestion(id);
		} catch (e) {
			console.error('Failed to accept suggestion:', e);
		}
	}

	async function dismiss(id: string) {
		try {
			await dismissLinkSuggestion(id);
			removeSuggestion(id);
		} catch (e) {
			console.error('Failed to dismiss suggestion:', e);
		}
	}

	function relationLabel(type: string): string {
		switch (type) {
			case 'supports': return 'Supports';
			case 'references': return 'References';
			case 'contradicts': return 'Contradicts';
			case 'continues': return 'Continues';
			case 'derivedfrom': return 'Derived from';
			default: return type;
		}
	}
</script>

{#if suggestions.visible}
	<div class="suggestion-panel">
		<div class="panel-header">
			<span class="panel-title">AI-Suggested Links</span>
			<button class="close-btn" onclick={() => toggleSuggestions()}>×</button>
		</div>

		{#if suggestions.pending.length === 0}
			<div class="empty">No pending suggestions</div>
		{:else}
			<div class="suggestion-list">
				{#each suggestions.pending as s (s.id)}
					<div class="suggestion-card">
						<div class="doc-pair">
							<span class="doc-name" title={s.fromTitle}>{s.fromTitle}</span>
							<span class="arrow">→</span>
							<span class="doc-name" title={s.toTitle}>{s.toTitle}</span>
						</div>
						<div class="meta">
							<span class="relation-badge">{relationLabel(s.relationType)}</span>
							<span class="strength">{Math.round(s.strength * 100)}%</span>
						</div>
						<div class="rationale">{s.rationale}</div>
						<div class="actions">
							<button class="btn-accept" onclick={() => accept(s.id)}>Accept</button>
							<button class="btn-dismiss" onclick={() => dismiss(s.id)}>Dismiss</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
{/if}

<style>
	.suggestion-panel {
		position: fixed;
		top: 120px;
		left: 16px;
		width: 320px;
		max-height: 400px;
		overflow-y: auto;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		z-index: 100;
		box-shadow: 0 4px 24px rgba(0, 0, 0, 0.4);
		display: flex;
		flex-direction: column;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
	}

	.panel-title {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 1.2rem;
		cursor: pointer;
		padding: 0 4px;
	}
	.close-btn:hover {
		color: var(--text-primary);
	}

	.empty {
		padding: 20px;
		text-align: center;
		color: var(--text-muted);
		font-size: 0.8rem;
	}

	.suggestion-list {
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.suggestion-card {
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 10px;
	}

	.doc-pair {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 6px;
	}

	.doc-name {
		font-size: 0.8rem;
		color: var(--text-primary);
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 120px;
	}

	.arrow {
		color: var(--text-muted);
		font-size: 0.75rem;
		flex-shrink: 0;
	}

	.meta {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 6px;
	}

	.relation-badge {
		font-size: 0.7rem;
		padding: 2px 6px;
		border-radius: 4px;
		background: var(--bg-tertiary);
		color: var(--accent);
		font-weight: 600;
	}

	.strength {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.rationale {
		font-size: 0.75rem;
		color: var(--text-secondary);
		line-height: 1.3;
		margin-bottom: 8px;
	}

	.actions {
		display: flex;
		gap: 6px;
	}

	.actions button {
		flex: 1;
		padding: 4px 8px;
		border-radius: 5px;
		font-size: 0.75rem;
		font-weight: 600;
		cursor: pointer;
		border: 1px solid var(--border);
	}

	.btn-accept {
		background: var(--success);
		color: #fff;
		border-color: var(--success) !important;
	}
	.btn-accept:hover {
		opacity: 0.85;
	}

	.btn-dismiss {
		background: transparent;
		color: var(--text-secondary);
	}
	.btn-dismiss:hover {
		background: var(--bg-hover);
	}
</style>
