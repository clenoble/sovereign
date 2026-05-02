<script lang="ts">
	/** AI chat panel wired into the mobile bottom sheet.
	 *
	 *  Detent behaviour:
	 *    peek    — state orb + last-message snippet + suggestion badge +
	 *              quick Approve/Reject buttons when a pending action exists
	 *    partial — scrollable message history + pinned input row
	 *    full    — same as partial, more vertical space
	 *
	 *  Auto-expands to partial on first message, to full when a pending
	 *  action requires explicit confirmation.
	 */
	import {
		chat,
		pushUser,
		pushSystem,
		clearGenerating,
		recentMessages
	} from '$lib/stores/chat.svelte';
	import { app, confirmPendingAction, rejectPendingAction } from '$lib/stores/app.svelte';
	import { suggestions, toggleSuggestions } from '$lib/stores/suggestions.svelte';
	import { chatMessage } from '$lib/api/commands';
	import { renderMarkdown } from '$lib/utils/markdown';
	import BottomSheet from './BottomSheet.svelte';

	let detent: 'peek' | 'partial' | 'full' = $state('peek');
	let inputValue = $state('');
	let messagesEl: HTMLDivElement | undefined = $state();
	let copiedIdx = $state<number | null>(null);

	let messages = $derived(recentMessages());
	let isActive = $derived(app.bubbleState !== 'Idle');

	function stateColor(s: string): string {
		switch (s) {
			case 'ProcessingOwned':
			case 'ProcessingExternal':
				return 'var(--bubble-processing, #6366f1)';
			case 'Executing':
				return 'var(--bubble-executing, #f59e0b)';
			case 'Proposing':
				return 'var(--bubble-proposing, #10b981)';
			case 'Suggesting':
				return 'var(--bubble-suggesting, #3b82f6)';
			default:
				return 'var(--bubble-idle, #555)';
		}
	}

	// Auto-scroll to bottom when messages arrive
	$effect(() => {
		messages;
		if (messagesEl) {
			setTimeout(() => {
				if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight;
			}, 0);
		}
	});

	// Auto-expand to partial when first message arrives
	$effect(() => {
		if (messages.length > 0 && detent === 'peek') {
			detent = 'partial';
		}
	});

	// Auto-expand to full for pending actions (high-gravity confirmation)
	$effect(() => {
		if (app.pendingAction && detent === 'peek') {
			detent = 'full';
		}
	});

	async function handleSubmit() {
		const text = inputValue.trim();
		if (!text) return;
		inputValue = '';

		if (app.pendingAction) {
			const lower = text.toLowerCase();
			if (['yes', 'y', 'go ahead', 'sure', 'ok', 'okay', 'approve'].includes(lower)) {
				pushUser(text);
				await confirmPendingAction();
				return;
			} else if (['no', 'n', 'cancel', 'reject', 'stop'].includes(lower)) {
				pushUser(text);
				await rejectPendingAction('User rejected via chat');
				return;
			}
			await rejectPendingAction('User provided new input');
		}

		pushUser(text);
		try {
			await chatMessage(text);
		} catch (e) {
			pushSystem(`Error: ${e}`);
			clearGenerating();
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
	}

	function rolePrefix(role: string) {
		if (role === 'user') return 'You';
		if (role === 'assistant') return 'AI';
		return '⚙';
	}

	function timeAgo(ts: number): string {
		const diff = Date.now() - ts;
		const mins = Math.floor(diff / 60000);
		if (mins < 1) return 'now';
		if (mins < 60) return `${mins}m`;
		const hrs = Math.floor(mins / 60);
		if (hrs < 24) return `${hrs}h`;
		return `${Math.floor(hrs / 24)}d`;
	}

	function provenanceClass(text: string): string {
		if (text.includes('(owned)')) return 'prov-owned';
		if (text.includes('(external)')) return 'prov-external';
		return '';
	}

	async function copyMessage(text: string, idx: number) {
		try {
			await navigator.clipboard.writeText(text);
			copiedIdx = idx;
			setTimeout(() => {
				copiedIdx = null;
			}, 1500);
		} catch {
			/* clipboard not available */
		}
	}

	function isInjectionWarning(text: string): boolean {
		return text.includes('Injection detected');
	}

	function lastSnippet(): string {
		if (!messages.length) return 'Chat with AI';
		const m = messages[messages.length - 1];
		const raw = m.text.replace(/<[^>]+>/g, '').trim();
		return raw.length > 48 ? raw.slice(0, 48) + '…' : raw;
	}
</script>

<BottomSheet bind:detent peekHeight={100}>
	{#snippet children()}
		{#if detent === 'peek'}
			<!-- ── Peek: state orb + snippet + badges + quick action buttons ── -->
			<div class="peek-row">
				<button
					class="state-orb"
					class:active={isActive}
					style="--orb-color: {stateColor(app.bubbleState)}"
					onclick={() => (detent = 'partial')}
					aria-label="Open AI chat"
				>
					<span class="orb-dot"></span>
				</button>

				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<span
					class="peek-label"
					onclick={() => (detent = 'partial')}
					onkeydown={(e) => e.key === 'Enter' && (detent = 'partial')}
				>
					{#if chat.generating}
						Thinking…
					{:else if app.pendingAction}
						Action pending &middot; {app.pendingAction.level}
					{:else}
						{lastSnippet()}
					{/if}
				</span>

				{#if suggestions.pending.length > 0}
					<button class="sugg-badge" onclick={toggleSuggestions} aria-label="View suggestions">
						{suggestions.pending.length}
					</button>
				{/if}

				{#if app.pendingAction}
					<button class="peek-approve" onclick={() => confirmPendingAction()} aria-label="Approve">
						&#10003;
					</button>
					<button
						class="peek-reject"
						onclick={() => rejectPendingAction('User rejected via button')}
						aria-label="Reject"
					>
						&#10005;
					</button>
				{/if}
			</div>
		{:else}
			<!-- ── Partial / Full: scrollable message history ── -->
			<div class="messages" bind:this={messagesEl}>
				{#each messages as msg, i}
					<div
						class="message {msg.role} {provenanceClass(msg.text)}"
						class:injection-warning={msg.role === 'system' && isInjectionWarning(msg.text)}
					>
						<div class="msg-header">
							<span class="prefix">{rolePrefix(msg.role)}</span>
							{#if msg.timestamp}
								<span class="timestamp">{timeAgo(msg.timestamp)}</span>
							{/if}
							{#if msg.role === 'assistant'}
								<button class="copy-btn" onclick={() => copyMessage(msg.text, i)} title="Copy">
									{copiedIdx === i ? '✓' : '⎘'}
								</button>
							{/if}
						</div>
						{#if msg.role === 'assistant'}
							<div class="text msg-markdown">{@html renderMarkdown(msg.text)}</div>
						{:else}
							<span class="text">{msg.text}</span>
						{/if}
					</div>
				{/each}

				{#if app.pendingAction}
					<div class="action-card">
						<p class="action-desc">{app.pendingAction.description}</p>
						<div class="action-btns">
							<button class="qr-approve" onclick={() => confirmPendingAction()}>Approve</button>
							<button class="qr-reject" onclick={() => rejectPendingAction('User rejected via button')}
								>Reject</button
							>
						</div>
					</div>
				{/if}

				{#if chat.generating}
					<div class="message system">
						<span class="prefix">&hellip;</span>
						<span class="text thinking">Thinking&hellip;</span>
					</div>
				{/if}
			</div>
		{/if}
	{/snippet}

	{#snippet footer()}
		<div class="input-row">
			<input
				type="text"
				placeholder="Type a message…"
				bind:value={inputValue}
				onkeydown={handleKeydown}
			/>
			<button class="send-btn" onclick={handleSubmit}>Send</button>
		</div>
	{/snippet}
</BottomSheet>

<style>
	/* ── Peek row ─────────────────────────────────────────────── */
	.peek-row {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 44px;
		padding: 2px 0;
	}

	.state-orb {
		flex-shrink: 0;
		width: 30px;
		height: 30px;
		border-radius: 50%;
		border: 2px solid var(--orb-color);
		background: none;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
	}
	.state-orb.active {
		animation: pulse-orb 2s ease-in-out infinite;
	}

	.orb-dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--orb-color);
	}

	@keyframes pulse-orb {
		0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--orb-color) 40%, transparent); }
		50%       { box-shadow: 0 0 0 5px transparent; }
	}

	.peek-label {
		flex: 1;
		font-size: 0.8rem;
		color: var(--text-secondary, #bbb);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		cursor: pointer;
	}

	.sugg-badge {
		flex-shrink: 0;
		min-width: 22px;
		height: 22px;
		border-radius: 11px;
		background: var(--accent, #f59e0b);
		color: #000;
		font-size: 0.65rem;
		font-weight: 700;
		border: none;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 5px;
	}

	.peek-approve,
	.peek-reject {
		flex-shrink: 0;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: none;
		font-size: 0.9rem;
		font-weight: 700;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
	}
	.peek-approve {
		background: var(--success, #10b981);
		color: #fff;
	}
	.peek-reject {
		background: transparent;
		border: 1px solid var(--border, #444);
		color: var(--text-muted, #888);
	}

	/* ── Messages ─────────────────────────────────────────────── */
	.messages {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding-right: 2px; /* room for scrollbar */
	}

	.message {
		font-size: 0.85rem;
		line-height: 1.45;
		word-wrap: break-word;
	}

	.msg-header {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 2px;
	}

	.prefix { font-weight: 600; }
	.message.user .prefix     { color: var(--chat-user, #a78bfa); }
	.message.assistant .prefix { color: var(--chat-assistant, #34d399); }
	.message.system .prefix   { color: var(--chat-system, #888); }
	.message.system .text     { color: var(--text-muted, #888); font-style: italic; }

	.timestamp {
		font-size: 0.7rem;
		color: var(--text-muted, #666);
	}

	.copy-btn {
		background: none;
		border: none;
		color: var(--text-muted, #666);
		cursor: pointer;
		font-size: 0.7rem;
		padding: 0 2px;
		opacity: 0;
		transition: opacity 0.15s;
	}
	.message:hover .copy-btn { opacity: 1; }

	.prov-owned   { border-left: 2px solid var(--prov-owned, #10b981); padding-left: 8px; }
	.prov-external { border-left: 2px solid var(--prov-external, #f59e0b); padding-left: 8px; }

	.msg-markdown :global(p)    { margin: 4px 0; }
	.msg-markdown :global(code) { background: rgba(255,255,255,0.08); padding: 1px 4px; border-radius: 3px; font-size: 0.8rem; }
	.msg-markdown :global(pre)  { background: rgba(0,0,0,0.3); padding: 8px 10px; border-radius: 6px; overflow-x: auto; margin: 6px 0; }
	.msg-markdown :global(pre code) { background: none; padding: 0; }
	.msg-markdown :global(ul), .msg-markdown :global(ol) { padding-left: 18px; margin: 4px 0; }
	.msg-markdown :global(a)   { color: var(--accent); }

	.thinking {
		animation: blink 1.5s ease-in-out infinite;
	}
	@keyframes blink {
		0%, 100% { opacity: 0.4; }
		50%       { opacity: 1; }
	}

	.injection-warning {
		background: color-mix(in srgb, var(--error, #ef4444) 10%, transparent);
		border-left: 3px solid var(--error, #ef4444);
		padding-left: 8px;
		border-radius: 4px;
	}
	.injection-warning .text {
		color: var(--error, #ef4444) !important;
		font-style: normal !important;
	}

	/* ── Pending action card ──────────────────────────────────── */
	.action-card {
		background: color-mix(in srgb, var(--accent, #f59e0b) 8%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent, #f59e0b) 30%, transparent);
		border-radius: 8px;
		padding: 10px 12px;
		margin-top: 4px;
	}
	.action-desc {
		font-size: 0.82rem;
		color: var(--text-secondary, #ccc);
		margin: 0 0 8px;
	}
	.action-btns {
		display: flex;
		gap: 8px;
	}
	.qr-approve {
		background: var(--success, #10b981);
		color: #fff;
		border: none;
		border-radius: 6px;
		padding: 9px 18px;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
		flex: 1;
	}
	.qr-approve:active { filter: brightness(0.85); }
	.qr-reject {
		background: transparent;
		color: var(--text-secondary, #bbb);
		border: 1px solid var(--border, #444);
		border-radius: 6px;
		padding: 9px 18px;
		font-size: 0.85rem;
		cursor: pointer;
		flex: 1;
	}
	.qr-reject:active { background: var(--bg-hover, #2a2a32); }

	/* ── Input row (footer slot) ──────────────────────────────── */
	.input-row {
		display: flex;
		gap: 8px;
	}

	.input-row input {
		flex: 1;
		background: var(--bg-input, #18181f);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		padding: 10px 12px;
		color: var(--text-primary, #e0e0e0);
		font-size: 0.9rem;
		font-family: inherit;
		outline: none;
	}
	.input-row input:focus {
		border-color: var(--accent, #f59e0b);
	}

	.send-btn {
		background: var(--accent, #f59e0b);
		color: #000;
		border: none;
		border-radius: 8px;
		padding: 10px 18px;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
		white-space: nowrap;
	}
	.send-btn:active { filter: brightness(0.88); }
</style>
