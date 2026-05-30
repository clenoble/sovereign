<script lang="ts">
	import {
		autofillPiiRecord,
		type BrowserFormExtraction,
		type PiiEntity,
		type PiiRecord
	} from '$lib/api/commands';
	import { piiState, refreshPiiRecords } from '$lib/stores/pii.svelte';
	import { focusTrap } from '$lib/actions/focusTrap';

	type Props = {
		open: boolean;
		extraction: BrowserFormExtraction | null;
		onClose: () => void;
	};

	let { open, extraction, onClose }: Props = $props();

	let selectedRecordId = $state<string | null>(null);
	let injecting = $state(false);
	let error = $state<string | null>(null);
	let lastInjected = $state<string | null>(null);

	$effect(() => {
		if (open) {
			selectedRecordId = null;
			injecting = false;
			error = null;
			lastInjected = null;
		}
	});

	// Extract host from the extraction's URL so we can match against
	// each entity's domains[].
	let host = $derived.by(() => {
		if (!extraction) return null;
		try {
			return new URL(extraction.url).host.toLowerCase();
		} catch {
			return null;
		}
	});

	// Find all entities whose domains[] contain the host (or a
	// parent domain — example.com matches mail.example.com).
	let matchingEntities = $derived.by(() => {
		if (!host) return [] as PiiEntity[];
		return piiState.entities.filter((e) =>
			e.domains.some((d) => domainMatches(host!, d.toLowerCase()))
		);
	});

	function domainMatches(host: string, entityDomain: string): boolean {
		const ed = entityDomain.replace(/^\./, '');
		return host === ed || host.endsWith('.' + ed);
	}

	// Vault entries (stored_secret=true) for the matching entities.
	let candidateRecords = $derived.by(() => {
		const ids = new Set(matchingEntities.map((e) => e.id));
		return piiState.records.filter(
			(r) => r.stored_secret && r.entity_id !== null && ids.has(r.entity_id)
		);
	});

	// The first password field's selector — what we'll inject into.
	let passwordSelector = $derived.by(() => {
		if (!extraction) return null;
		return extraction.fields.find((f) => f.kind === 'password')?.selector ?? null;
	});

	async function inject() {
		if (!selectedRecordId || !passwordSelector) {
			error = 'No record selected or no password field detected.';
			return;
		}
		injecting = true;
		error = null;
		try {
			await autofillPiiRecord(selectedRecordId, passwordSelector);
			const rec = candidateRecords.find((r: PiiRecord) => r.id === selectedRecordId);
			lastInjected = rec?.label ?? rec?.kind ?? 'credential';
			// Refresh metadata so last_revealed_at updates in the dashboard.
			refreshPiiRecords();
		} catch (e) {
			error = String(e);
		} finally {
			injecting = false;
		}
	}
</script>

{#if open && extraction}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="autofill-overlay" onclick={onClose}>
		<div
			class="autofill-dialog"
			role="dialog"
			aria-modal="true"
			aria-label="Fill credentials from vault"
			onclick={(e) => e.stopPropagation()}
			use:focusTrap={{ active: open, onEscape: onClose }}
		>
			<header class="autofill-header">
				<h3>Fill from vault</h3>
				<button type="button" class="close-btn" aria-label="Close" onclick={onClose}>
					&times;
				</button>
			</header>

			<p class="url-hint">
				On <code>{host ?? extraction.url}</code>
			</p>

			{#if lastInjected}
				<div class="success">
					Injected {lastInjected}. The password lives only in the form's input;
					it isn't logged anywhere else.
				</div>
				<footer class="autofill-footer">
					<button type="button" class="primary" onclick={onClose}>Done</button>
				</footer>
			{:else if !passwordSelector}
				<div class="warning">
					No password field detected on this page. Try the "Save credentials"
					flow if this is a signup form.
				</div>
				<footer class="autofill-footer">
					<button type="button" onclick={onClose}>Close</button>
				</footer>
			{:else if matchingEntities.length === 0}
				<div class="warning">
					No entity in your dashboard matches <code>{host}</code>. Use
					"Save credentials" to add one from this page.
				</div>
				<footer class="autofill-footer">
					<button type="button" onclick={onClose}>Close</button>
				</footer>
			{:else if candidateRecords.length === 0}
				<div class="warning">
					No vault entries saved for {matchingEntities[0].name}. Add one via
					the dashboard's "+ New secret" or via this page's "Save credentials".
				</div>
				<footer class="autofill-footer">
					<button type="button" onclick={onClose}>Close</button>
				</footer>
			{:else}
				<div class="record-picker">
					{#each candidateRecords as record (record.id)}
						<label class="record-row">
							<input
								type="radio"
								name="autofill-record"
								bind:group={selectedRecordId}
								value={record.id}
								disabled={injecting}
							/>
							<span class="record-info">
								<span class="record-kind">[{record.kind}]</span>
								<span class="record-label">
									{record.label ?? '(no label)'}
								</span>
								{#if record.last_revealed_at}
									<span class="record-meta">
										last used {new Date(record.last_revealed_at).toLocaleDateString()}
									</span>
								{/if}
							</span>
						</label>
					{/each}
				</div>

				{#if error}
					<div class="error">{error}</div>
				{/if}

				<footer class="autofill-footer">
					<button type="button" onclick={onClose} disabled={injecting}>Cancel</button>
					<button
						type="button"
						class="primary"
						onclick={inject}
						disabled={injecting || !selectedRecordId}
						title="Decrypts under your device key and types into the page (L3)"
					>
						{injecting ? 'Filling…' : 'Fill (L3 confirm)'}
					</button>
				</footer>
			{/if}
		</div>
	</div>
{/if}

<style>
	.autofill-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.autofill-dialog {
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 18px 22px;
		min-width: 420px;
		max-width: 560px;
		max-height: 70vh;
		overflow-y: auto;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
		display: flex;
		flex-direction: column;
		gap: 12px;
		color: var(--text-primary);
	}
	.autofill-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.autofill-header h3 {
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
	.url-hint,
	.warning,
	.success {
		margin: 0;
		font-size: 0.85rem;
	}
	.url-hint {
		color: var(--text-secondary);
	}
	.url-hint code,
	.warning code {
		font-family: monospace;
	}
	.warning {
		padding: 10px 14px;
		background: var(--bg-hover);
		border-left: 3px solid var(--text-secondary);
		border-radius: 4px;
	}
	.success {
		padding: 10px 14px;
		background: var(--bg-hover);
		border-left: 3px solid var(--accent);
		border-radius: 4px;
	}
	.record-picker {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.record-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		cursor: pointer;
	}
	.record-row:hover {
		background: var(--bg-hover);
	}
	.record-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 0.85rem;
	}
	.record-kind {
		font-weight: 500;
	}
	.record-label {
		color: var(--text-primary);
	}
	.record-meta {
		font-size: 0.7rem;
		color: var(--text-secondary);
	}
	.error {
		color: var(--error, #ef4444);
		font-size: 0.8rem;
	}
	.autofill-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}
	.autofill-footer button {
		padding: 6px 14px;
		border: 1px solid var(--border);
		background: var(--bg-hover);
		color: var(--text-primary);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.85rem;
	}
	.autofill-footer button.primary {
		background: var(--accent);
		color: #000;
		border-color: var(--accent);
	}
	.autofill-footer button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
