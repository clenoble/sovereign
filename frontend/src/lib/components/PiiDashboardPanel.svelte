<script lang="ts">
	import { onMount } from 'svelte';
	import { app } from '$lib/stores/app.svelte';
	import {
		piiState,
		loadPii,
		recordsForEntity,
		inventoryForEntity,
		piiCountForEntity,
		unreviewedCount
	} from '$lib/stores/pii.svelte';
	import type { PiiRecord, PiiEntity } from '$lib/api/commands';

	// Drag state — modeled on InboxPanel.
	let position = $state({ x: 80, y: 60 });
	let dragging = false;
	let dragStart = { x: 0, y: 0 };
	let dragOriginal = { x: 0, y: 0 };

	onMount(() => {
		if (!piiState.loaded) {
			loadPii();
		}
	});

	function selectEntity(id: string | null) {
		piiState.selectedEntityId = id;
		piiState.activeTab = 'inventory';
	}

	// Sort entities by descending PII count, with "Self" pinned to top.
	let sortedEntities = $derived.by(() => {
		const list = [...piiState.entities];
		list.sort((a, b) => {
			if (a.kind === 'self' && b.kind !== 'self') return -1;
			if (b.kind === 'self' && a.kind !== 'self') return 1;
			return piiCountForEntity(b.id) - piiCountForEntity(a.id);
		});
		return list;
	});

	// Records with no entity_id (typically Self-PII like the user's IBAN/AVS).
	let unattributedCount = $derived(
		piiState.records.filter((r) => r.entity_id === null && r.review_state !== 'dismissed')
			.length
	);

	// Selected entity object (or null when "Unattributed" is selected).
	let selectedEntity = $derived.by(() =>
		piiState.selectedEntityId === null
			? null
			: piiState.entities.find((e) => e.id === piiState.selectedEntityId) ?? null
	);

	// Records to show in the center column based on the active tab.
	let visibleRecords = $derived.by(() => {
		const all = recordsForEntity(piiState.selectedEntityId);
		switch (piiState.activeTab) {
			case 'inventory':
				return all.filter(
					(r) => !r.stored_secret && r.review_state !== 'dismissed'
				);
			case 'vault':
				return all.filter((r) => r.stored_secret);
			default:
				// Shared / cookies tabs land in later steps.
				return [] as PiiRecord[];
		}
	});

	// Drag handlers
	function handleHeaderPointerDown(e: PointerEvent) {
		if (e.button !== 0) return;
		dragging = true;
		dragStart = { x: e.clientX, y: e.clientY };
		dragOriginal = { x: position.x, y: position.y };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}
	function handleHeaderPointerMove(e: PointerEvent) {
		if (!dragging) return;
		position = {
			x: dragOriginal.x + (e.clientX - dragStart.x),
			y: dragOriginal.y + (e.clientY - dragStart.y)
		};
	}
	function handleHeaderPointerUp(e: PointerEvent) {
		dragging = false;
		(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
	}

	// Per-kind label for the column-1 record-count chip and the column-2 row.
	function kindLabel(kind: string): string {
		switch (kind) {
			case 'email':
				return 'Email';
			case 'phone':
				return 'Phone';
			case 'ssn':
				return 'SSN';
			case 'credit_card':
				return 'Card';
			case 'ipv4':
				return 'IP';
			case 'avs':
				return 'AVS';
			case 'iban':
				return 'IBAN';
			case 'passport':
				return 'Passport';
			case 'dob':
				return 'DOB';
			case 'address':
				return 'Address';
			case 'person_name':
				return 'Name';
			case 'org_name':
				return 'Org';
			case 'password':
				return 'Password';
			case 'api_token':
				return 'API Token';
			case 'bank_account':
				return 'Bank Account';
			case 'document_id':
				return 'Document ID';
			case 'note':
				return 'Note';
			default:
				return 'PII';
		}
	}

	function entityKindShape(kind: string): string {
		// Sovereignty Halo cue: rounded rect for Self, parallelogram for
		// external. Implemented via class so the CSS controls the actual
		// visual.
		return kind === 'self' ? 'shape-self' : 'shape-external';
	}
</script>

{#if app.piiDashboardVisible}
	<div
		class="pii-panel"
		role="dialog"
		aria-modal="false"
		aria-label="PII management dashboard"
		style="left: {position.x}px; top: {position.y}px;"
	>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="pii-header"
			onpointerdown={handleHeaderPointerDown}
			onpointermove={handleHeaderPointerMove}
			onpointerup={handleHeaderPointerUp}
		>
			<h3>
				PII Dashboard
				{#if unreviewedCount() > 0}
					<span class="header-badge">{unreviewedCount()} unreviewed</span>
				{/if}
			</h3>
			<button
				class="close-btn"
				aria-label="Close PII dashboard"
				onclick={() => (app.piiDashboardVisible = false)}>&times;</button
			>
		</div>

		<div class="pii-body">
			<!-- Column 1: Entity list -->
			<aside class="entity-column" aria-label="Entities">
				<button
					class="entity-row {piiState.selectedEntityId === null ? 'selected' : ''}"
					onclick={() => selectEntity(null)}
				>
					<span class="entity-shape shape-unattributed"></span>
					<span class="entity-info">
						<span class="entity-name">Unattributed</span>
						<span class="entity-kind">no entity link</span>
					</span>
					<span class="entity-count">{unattributedCount}</span>
				</button>

				{#each sortedEntities as entity (entity.id)}
					<button
						class="entity-row {piiState.selectedEntityId === entity.id ? 'selected' : ''}"
						onclick={() => selectEntity(entity.id)}
					>
						<span class="entity-shape {entityKindShape(entity.kind)}"></span>
						<span class="entity-info">
							<span class="entity-name">{entity.name}</span>
							<span class="entity-kind">{entity.kind}</span>
						</span>
						<span class="entity-count">{piiCountForEntity(entity.id)}</span>
					</button>
				{/each}

				{#if sortedEntities.length === 0 && unattributedCount === 0}
					<div class="empty">No entities yet</div>
				{/if}
			</aside>

			<!-- Column 2: Entity detail -->
			<section class="detail-column">
				<header class="detail-header">
					<h4>
						{selectedEntity ? selectedEntity.name : 'Unattributed'}
					</h4>
					<nav class="tab-row" aria-label="Entity tabs">
						<button
							class="tab {piiState.activeTab === 'inventory' ? 'active' : ''}"
							onclick={() => (piiState.activeTab = 'inventory')}
						>
							Inventory ({inventoryForEntity(piiState.selectedEntityId).length})
						</button>
						<button
							class="tab {piiState.activeTab === 'vault' ? 'active' : ''}"
							onclick={() => (piiState.activeTab = 'vault')}
						>
							Vault
						</button>
						<button
							class="tab {piiState.activeTab === 'shared' ? 'active' : ''}"
							onclick={() => (piiState.activeTab = 'shared')}
							disabled
							title="Sharing ledger lands in step 7"
						>
							Shared
						</button>
						<button
							class="tab {piiState.activeTab === 'cookies' ? 'active' : ''}"
							onclick={() => (piiState.activeTab = 'cookies')}
							disabled
							title="Cookies tab lands in step 8"
						>
							Cookies
						</button>
					</nav>
				</header>

				<div class="record-list">
					{#each visibleRecords as record (record.id)}
						<div class="record-row">
							<span class="record-kind">[{kindLabel(record.kind)}]</span>
							<span class="record-label">
								{record.label ?? '(no label)'}
							</span>
							<span class="record-meta">
								confidence {(record.confidence * 100).toFixed(0)}%
								{#if record.last_revealed_at}
									· revealed {new Date(record.last_revealed_at).toLocaleDateString()}
								{/if}
							</span>
							<span class="record-state state-{record.review_state}">
								{record.review_state}
							</span>
						</div>
					{/each}
					{#if visibleRecords.length === 0}
						<div class="empty">
							{#if piiState.activeTab === 'shared' || piiState.activeTab === 'cookies'}
								Coming in a later step.
							{:else}
								No {piiState.activeTab} entries.
							{/if}
						</div>
					{/if}
				</div>
			</section>

			<!-- Column 3: Review queue placeholder. Full UI lands in 6d. -->
			<aside class="review-column" aria-label="Review queue">
				<header class="review-header">
					<h4>Review queue</h4>
					<span class="review-count">{unreviewedCount()}</span>
				</header>
				<div class="review-placeholder">
					Confirm/dismiss UI lands in 6d.
				</div>
			</aside>
		</div>
	</div>
{/if}

<style>
	.pii-panel {
		position: fixed;
		width: 880px;
		max-height: 600px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		z-index: 90;
		display: flex;
		flex-direction: column;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
	}

	.pii-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
		cursor: grab;
		user-select: none;
	}
	.pii-header:active {
		cursor: grabbing;
	}
	.pii-header h3 {
		margin: 0;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-primary);
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.header-badge {
		font-size: 0.7rem;
		padding: 2px 8px;
		border-radius: 10px;
		background: var(--accent);
		color: #000;
		font-weight: 500;
	}
	.close-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 1.2rem;
		padding: 0 4px;
	}
	.close-btn:hover {
		color: var(--text-primary);
	}

	.pii-body {
		display: grid;
		grid-template-columns: 220px 1fr 240px;
		flex: 1;
		min-height: 0;
	}

	.entity-column {
		border-right: 1px solid var(--border);
		overflow-y: auto;
	}
	.entity-row {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 8px 12px;
		background: none;
		border: none;
		border-bottom: 1px solid var(--border);
		cursor: pointer;
		text-align: left;
		color: var(--text-primary);
	}
	.entity-row:hover {
		background: var(--bg-hover);
	}
	.entity-row.selected {
		background: var(--bg-selected, var(--bg-hover));
	}
	.entity-shape {
		width: 14px;
		height: 14px;
		flex-shrink: 0;
		background: var(--accent);
	}
	.entity-shape.shape-self {
		border-radius: 4px;
	}
	.entity-shape.shape-external {
		/* Parallelogram via skew. */
		transform: skewX(-20deg);
	}
	.entity-shape.shape-unattributed {
		border-radius: 50%;
		opacity: 0.5;
	}
	.entity-info {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.entity-name {
		font-size: 0.85rem;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.entity-kind {
		font-size: 0.7rem;
		color: var(--text-secondary);
	}
	.entity-count {
		font-size: 0.75rem;
		color: var(--text-secondary);
		min-width: 24px;
		text-align: right;
	}

	.detail-column {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.detail-header {
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
	}
	.detail-header h4 {
		margin: 0 0 8px 0;
		font-size: 0.95rem;
		color: var(--text-primary);
	}
	.tab-row {
		display: flex;
		gap: 4px;
	}
	.tab {
		background: none;
		border: 1px solid transparent;
		padding: 4px 10px;
		border-radius: 6px;
		font-size: 0.8rem;
		color: var(--text-secondary);
		cursor: pointer;
	}
	.tab:hover:not(:disabled) {
		color: var(--text-primary);
	}
	.tab.active {
		color: var(--text-primary);
		border-color: var(--border);
		background: var(--bg-hover);
	}
	.tab:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.record-list {
		flex: 1;
		overflow-y: auto;
	}
	.record-row {
		display: grid;
		grid-template-columns: 80px 1fr auto auto;
		align-items: center;
		gap: 10px;
		padding: 8px 14px;
		border-bottom: 1px solid var(--border);
		font-size: 0.82rem;
	}
	.record-kind {
		font-weight: 500;
		color: var(--text-primary);
	}
	.record-label {
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.record-meta {
		color: var(--text-secondary);
		font-size: 0.7rem;
	}
	.record-state {
		font-size: 0.7rem;
		padding: 2px 6px;
		border-radius: 4px;
	}
	.state-unreviewed {
		background: var(--accent);
		color: #000;
	}
	.state-confirmed {
		color: var(--text-secondary);
	}
	.state-dismissed {
		color: var(--text-secondary);
		text-decoration: line-through;
	}

	.review-column {
		border-left: 1px solid var(--border);
		display: flex;
		flex-direction: column;
	}
	.review-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
	}
	.review-header h4 {
		margin: 0;
		font-size: 0.9rem;
		color: var(--text-primary);
	}
	.review-count {
		font-size: 0.8rem;
		color: var(--text-secondary);
	}
	.review-placeholder {
		padding: 14px;
		font-size: 0.8rem;
		color: var(--text-secondary);
	}

	.empty {
		padding: 16px;
		text-align: center;
		color: var(--text-secondary);
		font-size: 0.85rem;
	}
</style>
