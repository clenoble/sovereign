<script lang="ts">
	import { modelPanelVisible } from '$lib/stores/app';
	import { scanModels, assignModelRole, deleteModel } from '$lib/api/commands';
	import type { ModelEntry } from '$lib/api/commands';
	import { onMount } from 'svelte';

	let models = $state<ModelEntry[]>([]);
	let loading = $state(false);
	let error = $state('');

	async function refresh() {
		loading = true;
		error = '';
		try {
			models = await scanModels();
		} catch (e) {
			error = String(e);
		}
		loading = false;
	}

	onMount(() => {
		refresh();
	});

	async function handleAssign(filename: string, role: string) {
		try {
			await assignModelRole(filename, role);
			await refresh();
		} catch (e) {
			error = String(e);
		}
	}

	async function handleDelete(filename: string) {
		if (!confirm(`Delete model "${filename}"?`)) return;
		try {
			await deleteModel(filename);
			await refresh();
		} catch (e) {
			error = String(e);
		}
	}

	function formatSize(mb: number): string {
		if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
		return `${mb.toFixed(0)} MB`;
	}
</script>

{#if $modelPanelVisible}
	<div class="model-panel">
		<div class="panel-header">
			<span class="panel-title">Models</span>
			<button class="close-btn" onclick={() => modelPanelVisible.set(false)}>&#x2715;</button>
		</div>

		{#if error}
			<p class="error">{error}</p>
		{/if}

		<div class="model-list">
			{#if loading}
				<p class="hint">Scanning...</p>
			{:else if models.length === 0}
				<p class="hint">No .gguf models found</p>
			{:else}
				{#each models as model}
					<div class="model-row">
						<div class="model-info">
							<span class="model-name" class:active-router={model.is_router} class:active-reasoning={model.is_reasoning}>
								{model.filename}
							</span>
							<span class="model-size">{formatSize(model.size_mb)}</span>
							{#if model.is_router}
								<span class="role-badge router">Router</span>
							{/if}
							{#if model.is_reasoning}
								<span class="role-badge reasoning">Reason</span>
							{/if}
						</div>
						<div class="model-actions">
							<button
								class="action-btn"
								disabled={model.is_router}
								onclick={() => handleAssign(model.filename, 'router')}
								title="Set as router model"
							>R</button>
							<button
								class="action-btn"
								disabled={model.is_reasoning}
								onclick={() => handleAssign(model.filename, 'reasoning')}
								title="Set as reasoning model"
							>Q</button>
							<button
								class="action-btn del"
								disabled={model.is_router || model.is_reasoning}
								onclick={() => handleDelete(model.filename)}
								title="Delete model"
							>Del</button>
						</div>
					</div>
				{/each}
			{/if}
		</div>

		<button class="refresh-btn" onclick={refresh}>Refresh</button>
	</div>
{/if}

<style>
	.model-panel {
		position: fixed;
		top: 60px;
		right: 16px;
		width: 340px;
		max-height: calc(100vh - 120px);
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
		z-index: 150;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
	}

	.panel-title {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.9rem;
	}
	.close-btn:hover {
		color: var(--error);
	}

	.error {
		color: var(--error);
		font-size: 0.8rem;
		padding: 6px 14px;
		margin: 0;
	}

	.model-list {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
	}

	.hint {
		color: var(--text-muted);
		font-size: 0.8rem;
		text-align: center;
		margin: 1rem 0;
	}

	.model-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 10px;
		border-radius: 6px;
	}
	.model-row:hover {
		background: var(--bg-hover);
	}

	.model-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.model-name {
		font-size: 0.8rem;
		color: var(--text-primary);
		word-break: break-all;
	}
	.model-name.active-router,
	.model-name.active-reasoning {
		color: var(--accent);
	}

	.model-size {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.role-badge {
		display: inline-block;
		font-size: 0.6rem;
		font-weight: 700;
		padding: 1px 5px;
		border-radius: 3px;
		text-transform: uppercase;
		width: fit-content;
	}
	.role-badge.router {
		background: var(--success);
		color: #000;
	}
	.role-badge.reasoning {
		background: var(--bubble-processing);
		color: #000;
	}

	.model-actions {
		display: flex;
		gap: 4px;
		flex-shrink: 0;
		margin-left: 8px;
	}

	.action-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 0.65rem;
		padding: 3px 7px;
		border-radius: 3px;
		cursor: pointer;
	}
	.action-btn:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--accent);
	}
	.action-btn:disabled {
		opacity: 0.3;
		cursor: default;
	}
	.action-btn.del:hover:not(:disabled) {
		border-color: var(--error);
		color: var(--error);
	}

	.refresh-btn {
		margin: 8px;
		padding: 6px;
		background: none;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-secondary);
		font-size: 0.8rem;
		cursor: pointer;
	}
	.refresh-btn:hover {
		border-color: var(--accent);
		color: var(--accent);
	}
</style>
