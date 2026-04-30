<script lang="ts">
	import {
		commitSignupCapture,
		generatePassword,
		type BrowserFormExtraction,
		type BrowserFormField,
		type SignupFieldInput
	} from '$lib/api/commands';
	import { refreshPiiRecords, loadPii } from '$lib/stores/pii.svelte';
	import { focusTrap } from '$lib/actions/focusTrap';

	type Props = {
		open: boolean;
		extraction: BrowserFormExtraction | null;
		onClose: () => void;
	};

	let { open, extraction, onClose }: Props = $props();

	// Editable copy of each captured field, with optional label.
	type Row = SignupFieldInput & { selector: string; placeholder: string; include: boolean };
	let rows = $state<Row[]>([]);
	let submitting = $state(false);
	let error = $state<string | null>(null);
	let lastResult = $state<{ entity_created: boolean; record_count: number } | null>(null);

	$effect(() => {
		if (open && extraction) {
			rows = extraction.fields.map((f: BrowserFormField) => ({
				kind: f.kind,
				value: f.value,
				label: defaultLabel(f),
				selector: f.selector,
				placeholder: f.placeholder,
				include: f.kind === 'password' || f.value.length > 0
			}));
			submitting = false;
			error = null;
			lastResult = null;
		}
	});

	function defaultLabel(f: BrowserFormField): string {
		if (f.label) return f.label;
		if (f.placeholder) return f.placeholder;
		return f.kind.replace('_', ' ');
	}

	async function generate(rowIdx: number) {
		try {
			rows[rowIdx].value = await generatePassword();
		} catch (e) {
			error = `Generate failed: ${e}`;
		}
	}

	async function submit(e: Event) {
		e.preventDefault();
		if (!extraction) return;
		const fields = rows
			.filter((r) => r.include && r.value.length > 0)
			.map((r) => ({ kind: r.kind, label: r.label || null, value: r.value }));
		if (fields.length === 0) {
			error = 'No fields selected to save.';
			return;
		}
		submitting = true;
		error = null;
		try {
			const result = await commitSignupCapture({
				url: extraction.url,
				entity_id: null, // auto-create from URL host if no match
				fields
			});
			lastResult = {
				entity_created: result.entity_created,
				record_count: result.record_ids.length
			};
			// Refresh dashboard data so the new vault entries + entity
			// + share records appear immediately.
			await loadPii();
			await refreshPiiRecords();
		} catch (e) {
			error = String(e);
		} finally {
			submitting = false;
		}
	}
</script>

{#if open && extraction}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="capture-overlay" onclick={onClose}>
		<form
			class="capture-dialog"
			role="dialog"
			aria-modal="true"
			aria-label="Save signup credentials"
			onclick={(e) => e.stopPropagation()}
			onsubmit={submit}
			use:focusTrap={{ active: open, onEscape: onClose }}
		>
			<header class="capture-header">
				<h3>Save signup credentials</h3>
				<button type="button" class="close-btn" aria-label="Close" onclick={onClose}>
					&times;
				</button>
			</header>

			<p class="url-hint">
				From <code>{extraction.url}</code>
			</p>

			{#if lastResult}
				<div class="success">
					Saved {lastResult.record_count} secret{lastResult.record_count === 1
						? ''
						: 's'}{lastResult.entity_created ? ' (new entity created)' : ''}.
				</div>
			{:else}
				{#each rows as row, i (row.selector)}
					<div class="field-row">
						<label class="include-cell">
							<input type="checkbox" bind:checked={row.include} disabled={submitting} />
						</label>
						<div class="field-body">
							<div class="field-meta">
								<span class="kind-badge">{row.kind}</span>
								<input
									class="label-input"
									type="text"
									bind:value={row.label}
									placeholder={row.placeholder || row.kind}
									disabled={submitting}
								/>
							</div>
							<div class="value-row">
								<input
									class="value-input"
									type={row.kind === 'password' ? 'password' : 'text'}
									bind:value={row.value}
									disabled={submitting}
								/>
								{#if row.kind === 'password'}
									<button
										type="button"
										class="generate-btn"
										onclick={() => generate(i)}
										disabled={submitting}
										title="Generate a 24-char password"
									>
										Generate
									</button>
								{/if}
							</div>
						</div>
					</div>
				{/each}

				{#if error}
					<div class="error">{error}</div>
				{/if}

				<footer class="capture-footer">
					<button type="button" onclick={onClose} disabled={submitting}>Cancel</button>
					<button type="submit" class="primary" disabled={submitting}>
						{submitting ? 'Saving…' : 'Save credentials'}
					</button>
				</footer>
			{/if}

			{#if lastResult}
				<footer class="capture-footer">
					<button type="button" class="primary" onclick={onClose}>Done</button>
				</footer>
			{/if}
		</form>
	</div>
{/if}

<style>
	.capture-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.capture-dialog {
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 18px 22px;
		min-width: 480px;
		max-width: 600px;
		max-height: 80vh;
		overflow-y: auto;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
		display: flex;
		flex-direction: column;
		gap: 12px;
		color: var(--text-primary);
	}
	.capture-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.capture-header h3 {
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
	.url-hint {
		margin: 0 0 4px 0;
		font-size: 0.78rem;
		color: var(--text-secondary);
	}
	.url-hint code {
		font-family: monospace;
		font-size: 0.78rem;
	}
	.success {
		padding: 10px 14px;
		background: var(--bg-hover);
		border-left: 3px solid var(--accent);
		border-radius: 4px;
		font-size: 0.85rem;
	}
	.field-row {
		display: flex;
		gap: 10px;
		align-items: flex-start;
		padding: 8px 0;
		border-bottom: 1px solid var(--border);
	}
	.include-cell {
		padding-top: 6px;
	}
	.field-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.field-meta {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.kind-badge {
		font-size: 0.7rem;
		padding: 2px 6px;
		background: var(--bg-hover);
		border-radius: 4px;
		color: var(--text-secondary);
	}
	.label-input {
		flex: 1;
		background: var(--bg-input, var(--bg-hover));
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 4px 8px;
		font-size: 0.82rem;
	}
	.value-row {
		display: flex;
		gap: 6px;
	}
	.value-input {
		flex: 1;
		background: var(--bg-input, var(--bg-hover));
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 6px 10px;
		font-size: 0.85rem;
		font-family: monospace;
	}
	.generate-btn {
		background: var(--bg-hover);
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 4px 10px;
		font-size: 0.78rem;
		cursor: pointer;
	}
	.generate-btn:hover:not(:disabled) {
		background: var(--bg-selected, var(--bg-hover));
	}
	.error {
		color: var(--error, #ef4444);
		font-size: 0.8rem;
	}
	.capture-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 4px;
	}
	.capture-footer button {
		padding: 6px 14px;
		border: 1px solid var(--border);
		background: var(--bg-hover);
		color: var(--text-primary);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.85rem;
	}
	.capture-footer button.primary {
		background: var(--accent);
		color: #000;
		border-color: var(--accent);
	}
	.capture-footer button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
