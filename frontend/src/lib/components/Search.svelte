<script lang="ts">
	import { searchVisible } from '$lib/stores/app';
	import { searchDocuments, searchQuery } from '$lib/api/commands';
	import type { SearchHit } from '$lib/api/commands';

	let query = $state('');
	let results = $state<SearchHit[]>([]);
	let searching = $state(false);
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		if (!$searchVisible) {
			query = '';
			results = [];
		}
	});

	function handleInput(e: Event) {
		query = (e.target as HTMLInputElement).value;

		// Debounce client-side search
		if (debounceTimer) clearTimeout(debounceTimer);
		if (query.trim().length > 0) {
			debounceTimer = setTimeout(async () => {
				searching = true;
				try {
					results = await searchDocuments(query);
				} catch {
					results = [];
				}
				searching = false;
			}, 150);
		} else {
			results = [];
		}
	}

	async function handleSubmit() {
		const q = query.trim();
		if (!q) return;
		// Send to AI orchestrator for full search
		try {
			await searchQuery(q);
		} catch (e) {
			console.error('Search query error:', e);
		}
		searchVisible.set(false);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			handleSubmit();
		} else if (e.key === 'Escape') {
			searchVisible.set(false);
		}
	}

	function selectResult(id: string) {
		// TODO Phase 3: navigate canvas to this document
		console.log('Navigate to document:', id);
		searchVisible.set(false);
	}

	function openResult(id: string) {
		// TODO Phase 2: open document panel
		console.log('Open document:', id);
		searchVisible.set(false);
	}
</script>

{#if $searchVisible}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="search-overlay" onkeydown={handleKeydown}>
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<div class="search-backdrop" onclick={() => searchVisible.set(false)}></div>
		<div class="search-modal">
			<input
				type="text"
				class="search-input"
				placeholder="Search documents... (Enter to ask AI)"
				value={query}
				oninput={handleInput}
				autofocus
			/>

			{#if results.length > 0}
				<div class="results">
					{#each results as hit}
						<div class="result-row">
							<button class="result-title" onclick={() => selectResult(hit.id)}>
								{hit.title}
							</button>
							<button class="result-open" onclick={() => openResult(hit.id)}>
								Open
							</button>
						</div>
					{/each}
					{#if results.length >= 50}
						<div class="more-hint">Showing first 50 results</div>
					{/if}
				</div>
			{:else if query.trim().length > 0 && !searching}
				<div class="no-results">No documents found</div>
			{/if}

			<div class="hint">
				{#if query.trim().length === 0}
					Type to search documents
				{:else}
					Press Enter to ask the AI assistant
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.search-overlay {
		position: fixed;
		inset: 0;
		z-index: 200;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 15vh;
	}

	.search-backdrop {
		position: absolute;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
	}

	.search-modal {
		position: relative;
		width: 480px;
		max-height: 60vh;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		overflow: hidden;
		box-shadow: 0 12px 48px rgba(0, 0, 0, 0.5);
	}

	.search-input {
		width: 100%;
		padding: 14px 18px;
		background: transparent;
		border: none;
		border-bottom: 1px solid var(--border);
		color: var(--text-primary);
		font-size: 1rem;
		outline: none;
		box-sizing: border-box;
	}

	.results {
		max-height: 320px;
		overflow-y: auto;
		padding: 4px 0;
	}

	.result-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 18px;
	}
	.result-row:hover {
		background: var(--bg-hover);
	}

	.result-title {
		flex: 1;
		text-align: left;
		background: none;
		border: none;
		color: var(--text-primary);
		font-size: 0.9rem;
		cursor: pointer;
		padding: 0;
	}
	.result-title:hover {
		color: var(--accent);
	}

	.result-open {
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-secondary);
		font-size: 0.75rem;
		padding: 2px 8px;
		cursor: pointer;
		margin-left: 8px;
	}
	.result-open:hover {
		border-color: var(--accent);
		color: var(--accent);
	}

	.no-results {
		padding: 16px 18px;
		color: var(--text-muted);
		font-size: 0.85rem;
	}

	.hint {
		padding: 10px 18px;
		color: var(--text-muted);
		font-size: 0.75rem;
		border-top: 1px solid var(--border);
	}

	.more-hint {
		padding: 8px 18px;
		color: var(--text-muted);
		font-size: 0.75rem;
		text-align: center;
	}
</style>
