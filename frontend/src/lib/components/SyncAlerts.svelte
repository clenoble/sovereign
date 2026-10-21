<script lang="ts">
	/** Alerts on changes from paired devices, the older-version notice, and
	 *  the previous values of synced items (spec ch. 05, C-WIRE 12–14).
	 *
	 *  A notice (desktop: a banner at the top; mobile: a banner over the
	 *  canvas) opens the Alerts panel (desktop) or sheet (mobile). Each alert
	 *  offers "Open the document", "Open the previous version" (where "Use
	 *  this version" puts it back) and "Dismiss" (changes nothing in the
	 *  document). Rows a sync replaced are listed as previous values, without
	 *  any pending badge.
	 *
	 *  Below them, the documents whose thread was deleted (S5-12, spec ch. 03,
	 *  *Thread deletion and the fallback lane*): each one, named with its
	 *  deleted thread, offers three ways out — move it to an existing thread,
	 *  put it in a new thread (named as in the new-thread gesture), or put it
	 *  in the trash — and each deleted thread offers the same for all its
	 *  documents. A status notice says how many wait, even with no alert; the
	 *  peer alerts are not changed by it (W6).
	 *
	 *  WCAG 2.1 AA: the notice is a live region (role="alert" for an alert,
	 *  role="status" for the older-version notice and the documents waiting);
	 *  the panel is a labelled dialog that takes focus when opened, closes on
	 *  Escape and gives focus back; every action is a real button with its own
	 *  label.
	 */
	import { tick } from 'svelte';
	import {
		dismissSyncAlert,
		restorePreviousValue,
		forgetPreviousValue,
		resolveOrphan,
		resolveOrphansOfThread,
		type OrphanResolutionDto
	} from '$lib/api/commands';
	import { canvas, refresh as refreshCanvas } from '$lib/stores/canvas.svelte';
	import { openById, openHistoryAt } from '$lib/stores/documents.svelte';
	import {
		syncAlerts,
		alertNotice,
		noticeText,
		olderNotice,
		removeAlert,
		removePreviousValue,
		openAlerts,
		closeAlerts,
		orphanNotice,
		orphansNotice,
		reloadOrphans,
		type Orphan,
		type SyncAlert,
		type PreviousValue
	} from '$lib/stores/syncAlerts.svelte';

	let { variant = 'desktop' }: { variant?: 'desktop' | 'mobile' } = $props();

	let busy = $state<string | null>(null);
	let error = $state('');
	let dialogEl = $state<HTMLElement | null>(null);
	let opener: HTMLElement | null = null;

	const notice = $derived(noticeText());
	const older = $derived(olderNotice());
	const waiting = $derived(orphansNotice());

	// ---- S5-12: documents whose thread was deleted ----

	/** The documents of one deleted thread. */
	interface OrphanGroup {
		threadId: string;
		label: string;
		docs: Orphan[];
	}

	const orphanGroups = $derived.by<OrphanGroup[]>(() => {
		const out: OrphanGroup[] = [];
		for (const o of syncAlerts.orphans) {
			let g = out.find((x) => x.threadId === o.threadId);
			if (!g) {
				g = {
					threadId: o.threadId,
					label: o.oldThreadName ? `All of “${o.oldThreadName}”` : 'All of the deleted thread',
					docs: []
				};
				out.push(g);
			}
			g.docs.push(o);
		}
		return out;
	});

	/** The open form: moving to a thread, or naming a new thread, for one
	 *  document or for all of a deleted thread. One at a time. */
	type Target = { kind: 'one'; docId: string } | { kind: 'group'; threadId: string };
	let form = $state<{ target: Target; mode: 'move' | 'new' } | null>(null);
	let chosenThread = $state('');
	let newName = $state('');

	function targetKey(t: Target): string {
		return t.kind === 'one' ? `one:${t.docId}` : `group:${t.threadId}`;
	}

	function isOpen(t: Target, mode: 'move' | 'new'): boolean {
		return form !== null && form.mode === mode && targetKey(form.target) === targetKey(t);
	}

	function openForm(target: Target, mode: 'move' | 'new') {
		error = '';
		form = { target, mode };
		chosenThread = canvas.threads[0]?.id ?? '';
		newName = '';
	}

	async function resolve(target: Target, resolution: OrphanResolutionDto) {
		const key = targetKey(target);
		busy = key;
		error = '';
		try {
			if (target.kind === 'one') await resolveOrphan(target.docId, resolution);
			else await resolveOrphansOfThread(target.threadId, resolution);
			form = null;
			await reloadOrphans();
			await refreshCanvas();
		} catch (e) {
			error = `Nothing was changed: ${e}`;
			await reloadOrphans();
		} finally {
			busy = null;
		}
	}

	function submitForm() {
		if (!form) return;
		if (form.mode === 'move') {
			if (!chosenThread) return;
			void resolve(form.target, { kind: 'move_to', thread_id: chosenThread });
		} else {
			const name = newName.trim();
			if (!name) return;
			void resolve(form.target, { kind: 'new_thread', name });
		}
	}

	/** A pasted newline becomes a space (THREAD-name N2). */
	function onNameInput(e: Event) {
		const el = e.currentTarget as HTMLInputElement;
		newName = el.value.replace(/[\r\n\t]+/g, ' ');
	}

	async function open(e: Event) {
		opener = e.currentTarget as HTMLElement;
		openAlerts();
		await tick();
		dialogEl?.focus();
	}

	function close() {
		closeAlerts();
		opener?.focus();
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
		}
	}

	async function openDocument(a: SyncAlert) {
		closeAlerts();
		await openById(a.docId);
	}

	async function openPrevious(a: SyncAlert) {
		if (!a.previousVersion) return;
		closeAlerts();
		await openHistoryAt(a.docId, a.previousVersion);
	}

	async function dismiss(a: SyncAlert) {
		busy = a.docId;
		error = '';
		try {
			await dismissSyncAlert(a.docId);
			removeAlert(a.docId);
		} catch (e) {
			error = `The alert could not be dismissed: ${e}`;
		} finally {
			busy = null;
		}
	}

	async function restore(v: PreviousValue) {
		busy = v.id;
		error = '';
		try {
			await restorePreviousValue(v.id);
			removePreviousValue(v.id);
		} catch (e) {
			error = `The previous value could not be restored: ${e}`;
		} finally {
			busy = null;
		}
	}

	async function forget(v: PreviousValue) {
		busy = v.id;
		error = '';
		try {
			await forgetPreviousValue(v.id);
			removePreviousValue(v.id);
		} catch (e) {
			error = `The item could not be removed from the list: ${e}`;
		} finally {
			busy = null;
		}
	}

	function when(iso: string): string {
		const d = new Date(iso);
		return isNaN(d.getTime()) ? iso : d.toLocaleString();
	}
</script>

{#if notice || older || waiting}
	<div class="notices {variant}">
		{#if notice}
			<div class="notice alert" role="alert">
				<span class="notice-text">{notice}</span>
				<button class="notice-btn" onclick={open} aria-haspopup="dialog">
					{syncAlerts.alerts.length === 1 ? 'See the alert' : 'See the alerts'}
				</button>
			</div>
		{/if}
		{#if waiting}
			<div class="notice waiting" role="status">
				<span class="notice-text">{waiting}</span>
				<button class="notice-btn" onclick={open} aria-haspopup="dialog">
					{syncAlerts.orphans.length === 1 ? 'See it' : 'See them'}
				</button>
			</div>
		{/if}
		{#if older}
			<div class="notice older" role="status">
				<span class="notice-text">{older}</span>
			</div>
		{/if}
	</div>
{/if}

{#snippet orphanForm(target: Target)}
	{#if isOpen(target, 'move')}
		<div class="orphan-form">
			{#if canvas.threads.length === 0}
				<p class="hint">There is no thread to move into: choose “New thread…” instead.</p>
			{:else}
				<label class="field-label" for="orphan-thread-choice">Thread</label>
				<select
					id="orphan-thread-choice"
					class="field"
					onchange={(e) => (chosenThread = (e.currentTarget as HTMLSelectElement).value)}
				>
					{#each canvas.threads as t (t.id)}
						<option value={t.id} selected={t.id === chosenThread}>{t.name}</option>
					{/each}
				</select>
				<div class="actions">
					<button class="btn primary" disabled={!chosenThread || busy !== null} onclick={submitForm}>Move</button>
					<button class="btn" onclick={() => (form = null)}>Cancel</button>
				</div>
			{/if}
		</div>
	{:else if isOpen(target, 'new')}
		<div class="orphan-form">
			<label class="field-label" for="orphan-thread-name">Thread name</label>
			<input
				id="orphan-thread-name"
				class="field"
				type="text"
				placeholder="Thread name…"
				value={newName}
				oninput={onNameInput}
				onkeydown={(e) => {
					if (e.key === 'Enter' && !e.isComposing) {
						e.preventDefault();
						submitForm();
					}
				}}
			/>
			<div class="actions">
				<button class="btn primary" disabled={newName.trim() === '' || busy !== null} onclick={submitForm}>Create</button>
				<button class="btn" onclick={() => (form = null)}>Cancel</button>
			</div>
		</div>
	{/if}
{/snippet}

{#if syncAlerts.open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="backdrop {variant}" onclick={close} onkeydown={onKeydown}></div>
	<div
		class="panel {variant}"
		role="dialog"
		aria-modal="true"
		aria-labelledby="sync-alerts-title"
		tabindex="-1"
		bind:this={dialogEl}
		onkeydown={onKeydown}
	>
		<div class="panel-header">
			<h2 id="sync-alerts-title" class="panel-title">Alerts</h2>
			<button class="close-btn" onclick={close} aria-label="Close the alerts">×</button>
		</div>

		{#if error}
			<p class="error" role="alert">{error}</p>
		{/if}

		{#if syncAlerts.alerts.length === 0}
			<p class="empty">No alerts. Changes from your devices apply on their own; one that looks suspicious shows up here.</p>
		{:else}
			<ul class="list" aria-label="Alerts">
				{#each syncAlerts.alerts as a (a.docId)}
					<li class="card">
						<span class="pill">Looks suspicious</span>
						<p class="card-title">{alertNotice(a)}</p>
						<p class="card-meta">Found: {a.finding}</p>
						<div class="actions">
							<button class="btn primary" onclick={() => openDocument(a)}>Open the document</button>
							{#if a.previousVersion}
								<button class="btn" onclick={() => openPrevious(a)}>Open the previous version</button>
							{/if}
							<button
								class="btn"
								disabled={busy === a.docId}
								onclick={() => dismiss(a)}
								aria-label="Dismiss the alert on {a.title || 'this document'}"
							>
								Dismiss
							</button>
						</div>
					</li>
				{/each}
			</ul>
			<p class="hint">Dismissing changes nothing in the document. To go back, open the previous version and choose "Use this version".</p>
		{/if}

		{#if syncAlerts.previousValues.length > 0}
			<h3 class="section-title" id="previous-values-title">Previous values</h3>
			<p class="hint">When a sync from one of your devices replaced an item, its earlier value was kept.</p>
			<ul class="list" aria-labelledby="previous-values-title">
				{#each syncAlerts.previousValues as v (v.id)}
					<li class="card">
						<p class="card-title">{v.label}</p>
						<p class="card-meta">Replaced by a sync from {v.device || 'a paired device'} · {when(v.at)}</p>
						<div class="actions">
							{#if v.canRestore}
								<button
									class="btn primary"
									disabled={busy === v.id}
									onclick={() => restore(v)}
									aria-label="Restore the previous value of {v.label}"
								>
									Restore
								</button>
							{/if}
							<button
								class="btn"
								disabled={busy === v.id}
								onclick={() => forget(v)}
								aria-label="Remove {v.label} from the list"
							>
								Remove from list
							</button>
						</div>
					</li>
				{/each}
			</ul>
		{/if}

		{#if syncAlerts.orphans.length > 0}
			<h3 class="section-title" id="orphans-title">Documents whose thread was deleted</h3>
			<p class="hint">
				Each waits for a decision, and stays on the canvas (in “Unfiled”) until then. The trash keeps a document 30
				days.
			</p>
			{#each orphanGroups as g (g.threadId)}
				<div class="orphan-group" role="group" aria-label={g.label}>
					{#if g.docs.length > 1}
						<p class="group-title">{g.label} ({g.docs.length} documents)</p>
						<div class="actions">
							<button class="btn" onclick={() => openForm({ kind: 'group', threadId: g.threadId }, 'move')}>
								Move all to a thread…
							</button>
							<button class="btn" onclick={() => openForm({ kind: 'group', threadId: g.threadId }, 'new')}>
								Put all in a new thread…
							</button>
							<button
								class="btn"
								disabled={busy !== null}
								onclick={() => resolve({ kind: 'group', threadId: g.threadId }, { kind: 'trash' })}
							>
								Put all in the trash
							</button>
						</div>
						{@render orphanForm({ kind: 'group', threadId: g.threadId })}
					{/if}
					<ul class="list" aria-labelledby="orphans-title">
						{#each g.docs as o (o.docId)}
							<li class="card orphan">
								<p class="card-title">{orphanNotice(o)}</p>
								<p class="card-meta">Last written on {o.device || 'a paired device'}</p>
								<div class="actions">
									<button class="btn primary" onclick={() => openForm({ kind: 'one', docId: o.docId }, 'move')}>
										Move to a thread…
									</button>
									<button class="btn" onclick={() => openForm({ kind: 'one', docId: o.docId }, 'new')}>New thread…</button>
									<button
										class="btn"
										disabled={busy !== null}
										onclick={() => resolve({ kind: 'one', docId: o.docId }, { kind: 'trash' })}
										aria-label="Put {o.title || 'this document'} in the trash"
									>
										Trash
									</button>
								</div>
								{@render orphanForm({ kind: 'one', docId: o.docId })}
							</li>
						{/each}
					</ul>
				</div>
			{/each}
		{/if}
	</div>
{/if}

<style>
	.notices {
		position: fixed;
		z-index: 95;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.notices.desktop {
		top: 12px;
		left: 50%;
		transform: translateX(-50%);
		max-width: min(720px, calc(100vw - 32px));
	}
	.notices.mobile {
		top: calc(env(safe-area-inset-top, 0px) + 56px);
		left: 8px;
		right: 8px;
	}
	.notice {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		border-radius: 8px;
		font-size: 0.85rem;
		box-shadow: 0 2px 10px rgba(0, 0, 0, 0.3);
	}
	/* Dark text on amber (#1a1a1a on #f59e0b: about 9:1). */
	.notice.alert {
		background: #f59e0b;
		color: #1a1a1a;
	}
	.notice.waiting {
		background: var(--bg-panel, #22222a);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid #818cf8;
	}
	.notice.waiting .notice-btn {
		background: #4f46e5;
	}
	.notice.older {
		background: var(--bg-panel, #22222a);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border, #444);
	}
	.notice-text {
		flex: 1;
	}
	.notice-btn {
		background: #1a1a1a;
		color: #fff;
		border: none;
		border-radius: 6px;
		padding: 6px 10px;
		font-size: 0.8rem;
		cursor: pointer;
		min-height: 32px;
	}
	.notice-btn:focus-visible,
	.btn:focus-visible,
	.close-btn:focus-visible {
		outline: 3px solid #fbbf24;
		outline-offset: 2px;
	}
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.35);
		z-index: 150;
	}
	.panel {
		position: fixed;
		z-index: 151;
		background: var(--bg-panel, #22222a);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border, #444);
		overflow-y: auto;
		padding: 12px 16px 16px;
		outline: none;
	}
	.panel.desktop {
		top: 60px;
		left: 50%;
		transform: translateX(-50%);
		width: min(640px, calc(100vw - 32px));
		max-height: calc(100vh - 120px);
		border-radius: 10px;
	}
	/* Mobile: a sheet from the bottom, above the taskbar. */
	.panel.mobile {
		left: 0;
		right: 0;
		bottom: 0;
		max-height: 80vh;
		border-radius: 14px 14px 0 0;
		padding-bottom: calc(16px + env(safe-area-inset-bottom, 0px));
	}
	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.panel-title {
		font-size: 1rem;
		margin: 4px 0 8px;
	}
	.section-title {
		font-size: 0.9rem;
		margin: 16px 0 4px;
	}
	.close-btn {
		background: none;
		border: none;
		color: var(--text-primary, #e0e0e0);
		font-size: 1.4rem;
		cursor: pointer;
		min-width: 36px;
		min-height: 36px;
	}
	.list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.card {
		border: 1px solid var(--border, #444);
		border-left: 3px solid #f59e0b;
		border-radius: 8px;
		padding: 10px 12px;
	}
	.pill {
		display: inline-block;
		background: #f59e0b;
		color: #1a1a1a;
		font-size: 0.7rem;
		font-weight: 700;
		border-radius: 5px;
		padding: 2px 8px;
	}
	.card-title {
		margin: 6px 0 2px;
		font-weight: 600;
	}
	.card-meta {
		margin: 0 0 8px;
		color: var(--text-secondary, #c8c8c8);
		font-size: 0.85rem;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.btn {
		background: var(--bg-input, #2c2c34);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border, #555);
		border-radius: 6px;
		padding: 6px 12px;
		cursor: pointer;
		min-height: 32px;
		font-size: 0.85rem;
	}
	/* White on indigo-600 (#4f46e5): 6.3:1. */
	.btn.primary {
		background: #4f46e5;
		border-color: #4f46e5;
		color: #fff;
	}
	.btn:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.hint,
	.empty {
		color: var(--text-secondary, #c8c8c8);
		font-size: 0.8rem;
		margin: 8px 0 0;
	}
	.error {
		color: #fca5a5;
		font-size: 0.85rem;
	}
	.card.orphan {
		border-left-color: #818cf8;
	}
	.orphan-group {
		margin-top: 10px;
	}
	.group-title {
		margin: 4px 0 6px;
		font-size: 0.85rem;
		color: var(--text-secondary, #c8c8c8);
	}
	.orphan-form {
		margin-top: 8px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.field-label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 0.8rem;
		color: var(--text-secondary, #c8c8c8);
	}
	.field {
		background: var(--bg-input, #2c2c34);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border, #555);
		border-radius: 6px;
		padding: 6px 8px;
		min-height: 32px;
		font-size: 0.9rem;
	}
	.field:focus-visible {
		outline: 3px solid #fbbf24;
		outline-offset: 1px;
	}
</style>
