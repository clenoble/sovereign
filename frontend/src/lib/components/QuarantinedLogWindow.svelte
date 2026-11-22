<!--
	C-LOG's host lot (spec ch. 08, DG3 and DG4; the Fable maille, by delegation of Céline,
	2026-11-18): what remains readable of a quarantined conversation log, read-only, for her
	only, with « Keep it » and « Delete it… ». The app has no Export (Céline, 2026-11-18:
	« Mettre l'export du téléphone au backlog »).

	Every entry is shown as plain text: Svelte escapes it, and nothing here renders markup.
	Closing the window keeps the log.
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { quarantinedLogReadable, deleteQuarantinedLog } from '$lib/api/commands';
	import type { ReadableLogDto } from '$lib/api/commands';
	import { pushSystem } from '$lib/stores/chat.svelte';
	import { focusTrap } from '$lib/actions/focusTrap';

	let { onclose }: { onclose: () => void } = $props();

	let readable = $state<ReadableLogDto | null>(null);
	let loaded = $state(false);
	let error = $state('');
	let confirming = $state(false);
	let cancelButton: HTMLButtonElement | undefined = $state();

	$effect(() => {
		load();
	});

	async function load() {
		try {
			readable = await quarantinedLogReadable();
		} catch (e) {
			error = String(e);
		}
		loaded = true;
	}

	/** The decided words for the entries from the break on (the Fable maille, 2026-11-18). */
	function notShown(n: number): string {
		return n === 1
			? '1 entry is not shown: from the break on, it no longer verifies.'
			: `${n} entries are not shown: from the break on, they no longer verify.`;
	}

	async function askDelete() {
		confirming = true;
		await tick();
		cancelButton?.focus();
	}

	async function confirmDelete() {
		confirming = false;
		try {
			await deleteQuarantinedLog(true);
		} catch (e) {
			error = String(e);
			return;
		}
		pushSystem('The quarantined log was deleted.');
		onclose();
	}
</script>

<div
	class="log-window"
	role="dialog"
	aria-modal="true"
	aria-label="What remains readable"
	use:focusTrap={{ active: true, onEscape: onclose }}
>
	<h2>What remains readable</h2>
	{#if error}
		<p class="error">{error}</p>
	{:else if !loaded}
		<p class="status">Loading…</p>
	{:else if !readable}
		<p class="status">No conversation log is quarantined.</p>
	{:else}
		{#if readable.order_not_guaranteed}
			<p class="note">These entries come from the older format: their order and completeness are not guaranteed.</p>
		{/if}
		<ol class="entries">
			{#each readable.entries as entry, i (i)}
				<li>{entry}</li>
			{/each}
		</ol>
		{#if readable.after_the_break > 0}
			<p class="note">{notShown(readable.after_the_break)}</p>
		{/if}
		{#each readable.earlier as earlier, i (i)}
			<p class="note">{earlier}</p>
		{/each}
	{/if}

	{#if confirming}
		<div class="confirm" role="alertdialog" aria-label="Delete this quarantined log?">
			<p>Delete this quarantined log? It cannot be brought back.</p>
			<button bind:this={cancelButton} onclick={() => (confirming = false)}>Cancel</button>
			<button class="danger" onclick={confirmDelete}>Delete</button>
		</div>
	{:else}
		<div class="choices">
			<button onclick={onclose}>Keep it</button>
			{#if readable}
				<button class="danger" onclick={askDelete}>Delete it…</button>
			{/if}
		</div>
	{/if}
</div>

<style>
	.log-window {
		position: fixed;
		inset: 10% 8%;
		z-index: 90;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 16px;
		overflow-y: auto;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		color: var(--text-primary);
	}
	h2 {
		margin: 0;
		font-size: 1rem;
	}
	.entries {
		margin: 0;
		padding-left: 1.4em;
		font-family: var(--font-mono, monospace);
		font-size: 0.8rem;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.note,
	.status {
		margin: 0;
		color: var(--text-muted);
		font-size: 0.8rem;
	}
	.error {
		color: var(--error, #ef4444);
	}
	.choices,
	.confirm {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		align-items: center;
	}
	.confirm p {
		flex-basis: 100%;
		margin: 0;
	}
	button {
		padding: 6px 12px;
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-secondary);
		cursor: pointer;
	}
	button.danger {
		border-color: var(--error, #ef4444);
		color: var(--error, #ef4444);
	}
</style>
