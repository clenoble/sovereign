<script lang="ts">
	import {
		peerReviews,
		removePeerReview,
		togglePeerReviews
	} from '$lib/stores/peerReviews.svelte';
	import { acceptPeerReview, restorePeerReview } from '$lib/api/commands';

	let busy = $state<string | null>(null);

	function key(kind: string, id: string): string {
		return `${kind}:${id}`;
	}

	async function accept(kind: string, id: string) {
		busy = key(kind, id);
		try {
			await acceptPeerReview(kind, id);
			removePeerReview(kind, id);
		} catch (e) {
			console.error('Failed to accept peer review:', e);
		} finally {
			busy = null;
		}
	}

	async function restore(kind: string, id: string) {
		busy = key(kind, id);
		try {
			await restorePeerReview(kind, id);
			removePeerReview(kind, id);
		} catch (e) {
			console.error('Failed to restore prior version:', e);
		} finally {
			busy = null;
		}
	}

	function riskClass(risk: string | undefined): string {
		switch (risk) {
			case 'high':
				return 'risk-high';
			case 'medium':
				return 'risk-medium';
			default:
				return 'risk-low';
		}
	}

	function riskLabel(risk: string | undefined): string {
		switch (risk) {
			case 'high':
				return 'High risk';
			case 'medium':
				return 'Review';
			case 'low':
				return 'Low risk';
			default:
				return 'Unreviewed';
		}
	}
</script>

{#if peerReviews.visible}
	<div class="review-panel">
		<div class="panel-header">
			<span class="panel-title">Synced changes to review</span>
			<button class="close-btn" onclick={() => togglePeerReviews()}>×</button>
		</div>

		{#if peerReviews.pending.length === 0}
			<div class="empty">No changes awaiting review</div>
		{:else}
			<div class="review-list">
				{#each peerReviews.pending as r (key(r.kind, r.id))}
					<div class="review-card">
						<div class="card-top">
							<span class="kind-badge">{r.kind}</span>
							<span class={'risk-badge ' + riskClass(r.assessment?.risk)}>
								{riskLabel(r.assessment?.risk)}
							</span>
						</div>
						<div class="title" title={r.title}>{r.title}</div>
						<div class="from">from {r.peer || 'a paired device'}</div>
						{#if r.assessment}
							<div class="summary">{r.assessment.summary}</div>
							{#if r.assessment.reasons.length > 0}
								<ul class="reasons">
									{#each r.assessment.reasons as reason}
										<li>{reason}</li>
									{/each}
								</ul>
							{/if}
							{#if !r.assessment.llm_assessed}
								<div class="note">Heuristic check only (no model verdict)</div>
							{/if}
						{/if}
						<div class="actions">
							<button
								class="btn-restore"
								disabled={!r.canRestore || busy === key(r.kind, r.id)}
								onclick={() => restore(r.kind, r.id)}
								title={r.canRestore
									? 'Revert to the version before this sync'
									: 'Restore not available for this item'}
							>
								Restore prior
							</button>
							<button
								class="btn-keep"
								disabled={busy === key(r.kind, r.id)}
								onclick={() => accept(r.kind, r.id)}
							>
								Keep synced
							</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
{/if}

<style>
	.review-panel {
		position: fixed;
		top: 120px;
		left: 16px;
		width: 340px;
		max-height: 420px;
		overflow-y: auto;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		z-index: 101;
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

	.review-list {
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.review-card {
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 10px;
	}

	.card-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 6px;
	}

	.kind-badge {
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		padding: 2px 6px;
		border-radius: 4px;
		background: var(--bg-tertiary);
		color: var(--text-muted);
		font-weight: 600;
	}

	.risk-badge {
		font-size: 0.7rem;
		padding: 2px 7px;
		border-radius: 4px;
		font-weight: 700;
	}
	.risk-high {
		background: var(--error, #ef4444);
		color: #fff;
	}
	.risk-medium {
		background: var(--warning, #f59e0b);
		color: #1a1a1a;
	}
	.risk-low {
		background: var(--bg-tertiary);
		color: var(--text-muted);
	}

	.title {
		font-size: 0.82rem;
		color: var(--text-primary);
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.from {
		font-size: 0.7rem;
		color: var(--text-muted);
		margin-bottom: 6px;
	}

	.summary {
		font-size: 0.75rem;
		color: var(--text-secondary);
		line-height: 1.3;
		margin-bottom: 4px;
	}

	.reasons {
		margin: 0 0 4px;
		padding-left: 16px;
	}
	.reasons li {
		font-size: 0.72rem;
		color: var(--text-secondary);
		line-height: 1.3;
	}

	.note {
		font-size: 0.68rem;
		color: var(--text-muted);
		font-style: italic;
		margin-bottom: 6px;
	}

	.actions {
		display: flex;
		gap: 6px;
		margin-top: 8px;
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
	.actions button:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.btn-restore {
		background: var(--accent, #6366f1);
		color: #fff;
		border-color: var(--accent, #6366f1) !important;
	}
	.btn-restore:not(:disabled):hover {
		opacity: 0.85;
	}

	.btn-keep {
		background: transparent;
		color: var(--text-secondary);
	}
	.btn-keep:not(:disabled):hover {
		background: var(--bg-hover);
	}
</style>
