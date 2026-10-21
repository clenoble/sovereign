<!--
	The trash, inside Settings (C-DOC adoption step 5, S5-11; decided by Céline
	2026-10-17). Each line: title, thread (or that it was deleted), days left,
	and "Restore". A deleted thread's documents are grouped, with "Restore all
	into a thread…"; restoring them asks which thread. "Empty trash" asks first.
	No per-item delete forever (Q1). No document content is shown.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { canvas, refresh as canvasRefresh } from '$lib/stores/canvas.svelte';
	import {
		trash,
		loadTrash,
		threadLabel,
		restore,
		deletedThreadGroups,
		restoreGroup,
		requestEmpty,
		cancelEmpty,
		confirmEmpty,
		type TrashLine
	} from '$lib/stores/trash.svelte';

	type Choice = { kind: 'one'; id: string; title: string } | { kind: 'group'; threadId: string; count: number };

	let choosing = $state<Choice | null>(null);
	let chosenThread = $state('');
	let threadSelect = $state<HTMLSelectElement | null>(null);
	let purged = $state<number | null>(null);

	const groups = $derived(deletedThreadGroups());
	const live = $derived(trash.lines.filter((l) => l.threadName !== null));
	const byId = (id: string) => trash.lines.find((l) => l.id === id);

	onMount(() => {
		void loadTrash();
	});

	function refreshCanvas() {
		canvasRefresh().catch(() => {});
	}

	async function restoreLine(line: TrashLine) {
		if (line.threadName === null) {
			choosing = { kind: 'one', id: line.id, title: line.title };
			chosenThread = canvas.threads[0]?.id ?? '';
			return;
		}
		if (await restore(line.id)) refreshCanvas();
	}

	function chooseForGroup(threadId: string, count: number) {
		choosing = { kind: 'group', threadId, count };
		chosenThread = canvas.threads[0]?.id ?? '';
	}

	async function confirmChoice() {
		const c = choosing;
		// Read the select itself: what is shown is what is chosen.
		const into = threadSelect?.value || chosenThread;
		if (!c || !into) return;
		choosing = null;
		const ok = c.kind === 'one' ? await restore(c.id, into) : await restoreGroup(c.threadId, into);
		if (ok) refreshCanvas();
	}

	async function doEmpty() {
		purged = await confirmEmpty();
	}

	function days(n: number): string {
		return n === 1 ? '1 day left' : `${n} days left`;
	}
</script>

<div class="trash">
	<h3>Trash ({trash.lines.length})</h3>
	<p class="hint">Deleted documents stay here for 30 days, then they are deleted for good.</p>

	{#if trash.error}
		<p class="error" role="alert">{trash.error}</p>
	{/if}
	{#if purged !== null}
		<p class="info" role="status">{purged} document{purged === 1 ? '' : 's'} deleted for good.</p>
	{/if}

	{#if trash.lines.length === 0}
		<p class="empty">The trash is empty.</p>
	{:else}
		{#each groups as g (g.threadId)}
			<section class="group">
				<div class="group-head">
					<span class="group-label">{g.label}</span>
					<button class="btn" onclick={() => chooseForGroup(g.threadId, g.ids.length)}>
						Restore all into a thread…
					</button>
				</div>
				<ul>
					{#each g.ids as id (id)}
						{@const line = byId(id)}
						{#if line}
							<li>
								<span class="title">{line.title}</span>
								<span class="meta">{threadLabel(line)} · {days(line.daysLeft)}</span>
								<button class="btn" aria-label="Restore {line.title}" onclick={() => restoreLine(line)}>Restore</button>
							</li>
						{/if}
					{/each}
				</ul>
			</section>
		{/each}

		{#if live.length > 0}
			<ul>
				{#each live as line (line.id)}
					<li>
						<span class="title">{line.title}</span>
						<span class="meta">{threadLabel(line)} · {days(line.daysLeft)}</span>
						<button class="btn" aria-label="Restore {line.title}" onclick={() => restoreLine(line)}>Restore</button>
					</li>
				{/each}
			</ul>
		{/if}

		<button class="btn danger" onclick={requestEmpty}>Empty trash</button>
	{/if}

	{#if choosing}
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="trash-choose-title">
			<h4 id="trash-choose-title">
				{choosing.kind === 'one' ? `Restore “${choosing.title}” into…` : `Restore ${choosing.count} documents into…`}
			</h4>
			{#if canvas.threads.length === 0}
				<p>There is no thread to restore into. Create a thread first, then restore.</p>
			{:else}
				<p>Its thread was deleted. To bring it back, create a new thread first, then choose it here.</p>
			{/if}
			<label>
				Thread
				<select bind:this={threadSelect} bind:value={chosenThread} aria-label="Thread">
					{#each canvas.threads as t (t.id)}
						<option value={t.id}>{t.name}</option>
					{/each}
				</select>
			</label>
			<div class="actions">
				<button class="btn" onclick={() => (choosing = null)}>Cancel</button>
				<button class="btn primary" disabled={!chosenThread} onclick={confirmChoice}>Restore</button>
			</div>
		</div>
	{/if}

	{#if trash.emptyRequested}
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="trash-empty-title">
			<h4 id="trash-empty-title">Empty the trash?</h4>
			<p>
				{trash.lines.length} document{trash.lines.length === 1 ? '' : 's'} will be deleted for good, with
				their versions. This cannot be undone.
			</p>
			<div class="actions">
				<button class="btn" onclick={cancelEmpty}>Cancel</button>
				<button class="btn danger" onclick={doEmpty}>Empty trash</button>
			</div>
		</div>
	{/if}
</div>

<style>
	.trash {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	h3 {
		margin: 0;
		font-size: 1rem;
	}
	.hint,
	.empty,
	.meta {
		color: var(--text-muted);
		font-size: 0.85rem;
	}
	.error {
		color: var(--error, #ef4444);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 0;
		border-bottom: 1px solid var(--border);
	}
	.title {
		flex: 1;
		color: var(--text-primary);
	}
	.group-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-top: 8px;
	}
	.group-label {
		font-weight: 600;
	}
	.btn {
		background: var(--bg-hover);
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 6px 10px;
		cursor: pointer;
	}
	.btn.primary {
		background: var(--accent, #f97316);
		color: #fff;
	}
	.btn.danger {
		color: var(--error, #ef4444);
		align-self: flex-start;
	}
	.dialog {
		position: fixed;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		z-index: 400;
		min-width: 280px;
		max-width: 90vw;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 16px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
	}
	.dialog label {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 12px;
	}
</style>
