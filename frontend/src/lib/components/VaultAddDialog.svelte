<script lang="ts">
	import { createVaultEntry } from '$lib/api/commands';
	import { piiState, refreshPiiRecords } from '$lib/stores/pii.svelte';
	import { focusTrap } from '$lib/actions/focusTrap';

	type Props = {
		open: boolean;
		onClose: () => void;
		/** Entity ID this secret belongs to. Falls back to the dashboard's
		 *  current selection. */
		defaultEntityId?: string | null;
	};

	let { open, onClose, defaultEntityId = null }: Props = $props();

	// Form state — reset on each open.
	let kind = $state('password');
	let label = $state('');
	let value = $state('');
	// Initialized to null and synced from `defaultEntityId` in the open
	// effect below (avoids capturing only the initial prop value).
	let entityId = $state<string | null>(null);
	let submitting = $state(false);
	let error = $state<string | null>(null);
	let showValue = $state(false);

	$effect(() => {
		if (open) {
			kind = 'password';
			label = '';
			value = '';
			entityId = defaultEntityId ?? piiState.selectedEntityId;
			submitting = false;
			error = null;
			showValue = false;
		}
	});

	async function submit(e: Event) {
		e.preventDefault();
		if (value.trim().length === 0) {
			error = 'Value is required';
			return;
		}
		submitting = true;
		error = null;
		try {
			await createVaultEntry({
				kind,
				label: label.trim() || null,
				entity_id: entityId,
				value
			});
			await refreshPiiRecords();
			onClose();
		} catch (e) {
			error = String(e);
		} finally {
			submitting = false;
		}
	}

	// Escape is handled by focusTrap's onEscape — kept here only for
	// the click-outside backdrop's keyboard fallback.
	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onClose();
		}
	}

	// Vault-style kinds — only the ones that make sense as user-entered
	// secrets (vs. discovered findings). Other PiiKind variants exist
	// but aren't useful here.
	const KINDS: Array<{ value: string; label: string }> = [
		{ value: 'password', label: 'Password' },
		{ value: 'api_token', label: 'API Token' },
		{ value: 'bank_account', label: 'Bank Account' },
		{ value: 'document_id', label: 'Document ID' },
		{ value: 'note', label: 'Secure Note' },
		{ value: 'iban', label: 'IBAN' },
		{ value: 'credit_card', label: 'Credit Card' },
		{ value: 'avs', label: 'AVS' },
		{ value: 'passport', label: 'Passport' },
		{ value: 'other', label: 'Other' }
	];
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="vault-overlay" onclick={onClose} onkeydown={handleKeydown}>
		<form
			class="vault-dialog"
			role="dialog"
			aria-modal="true"
			aria-label="New vault secret"
			onclick={(e) => e.stopPropagation()}
			onsubmit={submit}
			use:focusTrap={{ active: open, onEscape: onClose }}
		>
			<header class="vault-dialog-header">
				<h3>New secret</h3>
				<button type="button" class="close-btn" aria-label="Close" onclick={onClose}>
					&times;
				</button>
			</header>

			<label class="field">
				<span class="field-label">Kind</span>
				<select bind:value={kind} disabled={submitting}>
					{#each KINDS as k (k.value)}
						<option value={k.value}>{k.label}</option>
					{/each}
				</select>
			</label>

			<label class="field">
				<span class="field-label">Label <span class="optional">(optional)</span></span>
				<input
					type="text"
					bind:value={label}
					placeholder="e.g. main bank password"
					disabled={submitting}
				/>
			</label>

			<label class="field">
				<span class="field-label">Entity</span>
				<select bind:value={entityId} disabled={submitting}>
					<option value={null}>Unattributed</option>
					{#each piiState.entities as entity (entity.id)}
						<option value={entity.id}>{entity.name} ({entity.kind})</option>
					{/each}
				</select>
			</label>

			<label class="field">
				<span class="field-label">Value</span>
				<div class="value-input-wrapper">
					<input
						type={showValue ? 'text' : 'password'}
						bind:value
						placeholder="encrypted under your device key"
						autocomplete="new-password"
						disabled={submitting}
					/>
					<button
						type="button"
						class="reveal-btn"
						aria-label={showValue ? 'Hide value' : 'Show value'}
						aria-pressed={showValue}
						onclick={() => (showValue = !showValue)}
						disabled={submitting}
					>
						{showValue ? '🙈' : '👁'}
					</button>
				</div>
			</label>

			{#if error}
				<div class="error">{error}</div>
			{/if}

			<footer class="vault-dialog-footer">
				<button type="button" onclick={onClose} disabled={submitting}>Cancel</button>
				<button type="submit" class="primary" disabled={submitting}>
					{submitting ? 'Saving…' : 'Save secret'}
				</button>
			</footer>
		</form>
	</div>
{/if}

<style>
	.vault-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.vault-dialog {
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 18px 22px;
		min-width: 360px;
		max-width: 480px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
		display: flex;
		flex-direction: column;
		gap: 12px;
		color: var(--text-primary);
	}

	.vault-dialog-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.vault-dialog-header h3 {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 1.4rem;
		line-height: 1;
		padding: 0 4px;
	}
	.close-btn:hover {
		color: var(--text-primary);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.field-label {
		font-size: 0.75rem;
		color: var(--text-secondary);
		font-weight: 500;
	}

	.optional {
		opacity: 0.6;
		font-weight: 400;
	}

	.field input,
	.field select {
		background: var(--bg-input, var(--bg-hover));
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 6px 10px;
		font-size: 0.85rem;
	}

	.field input:focus,
	.field select:focus {
		outline: 1px solid var(--accent);
		border-color: var(--accent);
	}

	.value-input-wrapper {
		display: flex;
		gap: 6px;
	}
	.value-input-wrapper input {
		flex: 1;
	}
	.reveal-btn {
		background: var(--bg-input, var(--bg-hover));
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0 10px;
		cursor: pointer;
		color: var(--text-primary);
		font-size: 0.95rem;
		line-height: 1;
	}
	.reveal-btn:hover:not(:disabled) {
		background: var(--bg-hover);
	}
	.reveal-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error {
		color: var(--error, #ef4444);
		font-size: 0.8rem;
	}

	.vault-dialog-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 4px;
	}

	.vault-dialog-footer button {
		padding: 6px 14px;
		border: 1px solid var(--border);
		background: var(--bg-hover);
		color: var(--text-primary);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.85rem;
	}

	.vault-dialog-footer button:hover:not(:disabled) {
		background: var(--bg-selected, var(--bg-hover));
	}

	.vault-dialog-footer button.primary {
		background: var(--accent);
		color: #000;
		border-color: var(--accent);
	}

	.vault-dialog-footer button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
