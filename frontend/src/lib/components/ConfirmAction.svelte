<script lang="ts">
	import { pendingAction } from '$lib/stores/app';
	import { approveAction, rejectAction } from '$lib/api/commands';
	import { chat } from '$lib/stores/chat';

	async function handleApprove() {
		chat.pushSystem('Approved.');
		pendingAction.set(null);
		try {
			await approveAction();
		} catch (e) {
			chat.pushSystem(`Approve error: ${e}`);
		}
	}

	async function handleReject() {
		chat.pushSystem('Rejected.');
		pendingAction.set(null);
		try {
			await rejectAction('User rejected via UI');
		} catch (e) {
			chat.pushSystem(`Reject error: ${e}`);
		}
	}
</script>

{#if $pendingAction}
	<div class="confirm-overlay">
		<div class="confirm-backdrop"></div>
		<div class="confirm-dialog">
			<div class="confirm-header">
				<span class="level-badge">{$pendingAction.level}</span>
				<span class="confirm-title">Action Confirmation</span>
			</div>
			<p class="confirm-desc">{$pendingAction.description}</p>
			<div class="confirm-actions">
				<button class="btn approve" onclick={handleApprove}>Approve</button>
				<button class="btn reject" onclick={handleReject}>Reject</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.confirm-overlay {
		position: fixed;
		inset: 0;
		z-index: 300;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.confirm-backdrop {
		position: absolute;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
	}

	.confirm-dialog {
		position: relative;
		width: 380px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 20px;
		box-shadow: 0 12px 48px rgba(0, 0, 0, 0.5);
	}

	.confirm-header {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 14px;
	}

	.level-badge {
		background: var(--bubble-proposing);
		color: #000;
		font-size: 0.7rem;
		font-weight: 700;
		padding: 2px 8px;
		border-radius: 4px;
		text-transform: uppercase;
	}

	.confirm-title {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.confirm-desc {
		color: var(--text-secondary);
		font-size: 0.85rem;
		line-height: 1.5;
		margin: 0 0 18px;
	}

	.confirm-actions {
		display: flex;
		gap: 10px;
		justify-content: flex-end;
	}

	.btn {
		padding: 8px 18px;
		border: none;
		border-radius: 6px;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
	}

	.approve {
		background: var(--success);
		color: #fff;
	}
	.approve:hover {
		filter: brightness(1.15);
	}

	.reject {
		background: var(--error);
		color: #fff;
	}
	.reject:hover {
		filter: brightness(1.15);
	}
</style>
