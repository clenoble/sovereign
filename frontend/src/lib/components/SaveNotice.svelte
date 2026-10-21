<script lang="ts">
	/** Why the last save of a document panel was refused; that a save kept
	 *  both versions (the document had changed elsewhere: this text was saved,
	 *  the other version is in History); and — only if that save conflicted
	 *  again — the user's two choices. */
	import {
		type OpenPanel,
		keepMine,
		loadStored,
		dismissSaveError,
		dismissSaveInfo
	} from '$lib/stores/documents.svelte';

	let { panel }: { panel: OpenPanel } = $props();
</script>

{#if panel.saveInfo && !panel.saveError}
	<div class="save-notice" role="status">
		<p class="msg">{panel.saveInfo}</p>
		<div class="actions">
			<button onclick={() => dismissSaveInfo(panel.doc.id)}>Dismiss</button>
		</div>
	</div>
{/if}
{#if panel.saveError}
	<div class="save-notice" role="alert">
		<p class="msg">{panel.saveError}</p>
		{#if panel.conflict}
			<p class="hint">
				Your text is kept here. Keeping it saves it over the stored version, which stays in History.
			</p>
			<div class="actions">
				<button onclick={() => keepMine(panel.doc.id)}>Keep my version</button>
				<button onclick={() => loadStored(panel.doc.id)}>Load the stored version</button>
			</div>
		{:else}
			<div class="actions">
				<button onclick={() => dismissSaveError(panel.doc.id)}>Dismiss</button>
			</div>
		{/if}
	</div>
{/if}

<style>
	.save-notice {
		margin: 6px 12px;
		padding: 8px 10px;
		border: 1px solid var(--warning, #d9a441);
		border-radius: 6px;
		background: var(--bg-panel, #22222a);
		color: var(--text-primary, #e8e8ee);
		font-size: 0.85rem;
	}
	.msg {
		margin: 0 0 4px;
	}
	.hint {
		margin: 0 0 6px;
		color: var(--text-secondary, #b8b8c4);
	}
	.actions {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
	.actions button {
		min-height: 32px;
		padding: 4px 10px;
		border-radius: 4px;
		border: 1px solid var(--border, #44444f);
		background: var(--bg-secondary, #2a2a33);
		color: inherit;
		cursor: pointer;
	}
	.actions button:focus-visible {
		outline: 2px solid var(--accent, #6aa0ff);
		outline-offset: 2px;
	}
</style>
