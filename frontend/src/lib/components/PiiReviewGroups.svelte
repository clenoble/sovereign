<script lang="ts">
	/** C-SWEEP SW5 on the phone (spec ch. 09, lot LS(a)): the review window,
	 *  grouped by type, from `piiReview` (`$lib/stores/piiReview.svelte`).
	 *  Each row: a checkbox whose accessible name holds the row's state in
	 *  words; the state in words on the row. Gestures act on the records
	 *  selected: « Confirm selected », « Dismiss selected »; none acts on a
	 *  whole type (Céline, 2026-11-17).
	 *
	 *  Signature by the test author of lot LS(a); the body is the implementer's. */
	import {
		piiReview,
		toggleSelected,
		confirmSelected,
		dismissSelected,
		takeBackDismissalSelected,
		takeBackConfirmationSelected
	} from '$lib/stores/piiReview.svelte';

	let busy = $state(false);
	let error = $state<string | null>(null);

	const none = $derived(piiReview.selected.length === 0);

	/** How a row is named: its type, where it is found, and its state. */
	function rowName(label: string, documents: string[], words: string): string {
		const n = documents.length;
		return `${label}, in ${n} document${n === 1 ? '' : 's'}: ${words}`;
	}

	async function run(gesture: () => Promise<void>) {
		busy = true;
		error = null;
		try {
			await gesture();
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}
</script>

<div class="review">
	{#if piiReview.loaded && piiReview.groups.length === 0}
		<p class="empty">No personal data detected yet.</p>
	{/if}
	{#each piiReview.groups as group (group.kind)}
		<section role="group" aria-labelledby={`pii-group-${group.kind}`}>
			<h3 id={`pii-group-${group.kind}`}>{group.label}</h3>
			<ul>
				{#each group.rows as row (row.record_id)}
					<li>
						<label>
							<input
								type="checkbox"
								checked={piiReview.selected.includes(row.record_id)}
								aria-label={rowName(group.label, row.documents, row.state_words)}
								onchange={() => toggleSelected(row.record_id)}
							/>
							<span class="masked" aria-hidden="true">••••••</span>
							<span class="state">{row.state_words}</span>
						</label>
					</li>
				{/each}
			</ul>
		</section>
	{/each}
	<div class="gestures">
		<button disabled={busy || none} onclick={() => run(confirmSelected)}>Confirm selected</button>
		<button disabled={busy || none} onclick={() => run(dismissSelected)}>Dismiss selected</button>
		<button disabled={busy || none} onclick={() => run(takeBackDismissalSelected)}>Take back dismissal</button>
		<button disabled={busy || none} onclick={() => run(takeBackConfirmationSelected)}>Take back confirmation</button>
	</div>
	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}
</div>

<style>
	.review {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	h3 {
		margin: 0 0 6px;
		font-size: 0.95rem;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li label {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 44px;
	}
	.masked {
		color: var(--text-secondary, #aaa);
	}
	.state {
		font-size: 0.85rem;
	}
	.gestures {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.gestures button {
		min-height: 44px;
	}
	.error {
		color: var(--danger, #f87171);
	}
</style>
