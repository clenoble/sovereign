<script lang="ts">
	import { chat, pushUser, pushSystem, clearGenerating, toggleChat, recentMessages } from '$lib/stores/chat.svelte';
	import { app, confirmPendingAction, rejectPendingAction } from '$lib/stores/app.svelte';
	import { chatMessage } from '$lib/api/commands';
	import { renderMarkdown } from '$lib/utils/markdown';

	let inputValue = $state('');
	let messagesEl: HTMLDivElement | undefined = $state();
	let copiedIdx = $state<number | null>(null);

	let messages = $derived(recentMessages());

	function scrollToBottom() {
		if (messagesEl) {
			messagesEl.scrollTop = messagesEl.scrollHeight;
		}
	}

	// Scroll when new messages arrive
	$effect(() => {
		messages;
		// Use setTimeout to wait for DOM update
		setTimeout(scrollToBottom, 0);
	});

	async function handleSubmit() {
		const text = inputValue.trim();
		if (!text) return;
		inputValue = '';

		// Check if there's a pending action — handle confirmation via chat
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
			// Any other input cancels the pending action and processes normally
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
		return '\u2699'; // gear icon for system
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
			setTimeout(() => { copiedIdx = null; }, 1500);
		} catch { /* clipboard not available */ }
	}

	const handleQuickApprove = () => confirmPendingAction();
	const handleQuickReject = () => rejectPendingAction('User rejected via button');

	function isInjectionWarning(text: string): boolean {
		return text.includes('Injection detected');
	}
</script>

{#if chat.visible}
	<div class="chat-panel">
		<div class="chat-header">
			<span class="chat-title">Chat</span>
			<button class="close-btn" onclick={() => toggleChat()}>X</button>
		</div>

		<div class="messages" bind:this={messagesEl}>
			{#each messages as msg, i}
				<div class="message {msg.role} {provenanceClass(msg.text)}" class:injection-warning={msg.role === 'system' && isInjectionWarning(msg.text)}>
					<div class="msg-header">
						<span class="prefix">{rolePrefix(msg.role)}</span>
						{#if msg.timestamp}
							<span class="timestamp">{timeAgo(msg.timestamp)}</span>
						{/if}
						{#if msg.role === 'assistant'}
							<button class="copy-btn" onclick={() => copyMessage(msg.text, i)} title="Copy">
								{copiedIdx === i ? '\u2713' : '\u2398'}
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
				<div class="quick-reply">
					<button class="qr-approve" onclick={handleQuickApprove}>Approve</button>
					<button class="qr-reject" onclick={handleQuickReject}>Reject</button>
				</div>
			{/if}

			{#if chat.generating}
				<div class="message system">
					<span class="prefix">&hellip;</span>
					<span class="text thinking">Thinking...</span>
				</div>
			{/if}
		</div>

		<div class="input-row">
			<input
				type="text"
				placeholder="Type a message..."
				bind:value={inputValue}
				onkeydown={handleKeydown}
			/>
			<button class="send-btn" onclick={handleSubmit}>Send</button>
		</div>
	</div>
{/if}

<style>
	.chat-panel {
		position: fixed;
		top: 120px;
		left: 16px;
		width: 420px;
		max-height: 480px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		display: flex;
		flex-direction: column;
		z-index: 100;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
	}

	.chat-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
	}

	.chat-title {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.85rem;
		padding: 2px 6px;
	}
	.close-btn:hover {
		color: var(--text-primary);
	}

	.messages {
		flex: 1;
		overflow-y: auto;
		padding: 10px 14px;
		max-height: 340px;
	}

	.message {
		margin-bottom: 8px;
		font-size: 0.85rem;
		line-height: 1.4;
		word-wrap: break-word;
	}

	.prefix {
		font-weight: 600;
		margin-right: 6px;
	}

	.message.user .prefix {
		color: var(--chat-user);
	}
	.message.assistant .prefix {
		color: var(--chat-assistant);
	}
	.message.system .prefix {
		color: var(--chat-system);
	}
	.message.system .text {
		color: var(--text-muted);
		font-style: italic;
	}

	.msg-header {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 2px;
	}

	.timestamp {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.copy-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.7rem;
		padding: 0 2px;
		opacity: 0;
		transition: opacity 0.15s;
	}
	.message:hover .copy-btn {
		opacity: 1;
	}
	.copy-btn:hover {
		color: var(--text-primary);
	}

	.msg-markdown :global(p) {
		margin: 4px 0;
	}
	.msg-markdown :global(code) {
		background: rgba(255, 255, 255, 0.08);
		padding: 1px 4px;
		border-radius: 3px;
		font-size: 0.8rem;
	}
	.msg-markdown :global(pre) {
		background: rgba(0, 0, 0, 0.3);
		padding: 8px 10px;
		border-radius: 6px;
		overflow-x: auto;
		margin: 6px 0;
	}
	.msg-markdown :global(pre code) {
		background: none;
		padding: 0;
	}
	.msg-markdown :global(ul), .msg-markdown :global(ol) {
		padding-left: 18px;
		margin: 4px 0;
	}
	.msg-markdown :global(a) {
		color: var(--accent);
	}
	.msg-markdown :global(.escaped-html) {
		display: block;
		color: var(--text-muted);
		font-size: 0.75em;
		font-family: monospace;
	}

	.prov-owned {
		border-left: 2px solid var(--prov-owned);
		padding-left: 8px;
	}
	.prov-external {
		border-left: 2px solid var(--prov-external);
		padding-left: 8px;
	}

	.thinking {
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 0.4;
		}
		50% {
			opacity: 1;
		}
	}

	.input-row {
		display: flex;
		gap: 8px;
		padding: 10px 14px;
		border-top: 1px solid var(--border);
	}

	.input-row input {
		flex: 1;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 8px 10px;
		color: var(--text-primary);
		font-size: 0.85rem;
		outline: none;
	}
	.input-row input:focus {
		border-color: var(--accent);
	}

	.send-btn {
		background: var(--accent);
		color: #000;
		border: none;
		border-radius: 6px;
		padding: 8px 14px;
		font-size: 0.8rem;
		font-weight: 600;
		cursor: pointer;
	}
	.send-btn:hover {
		background: var(--accent-hover);
	}

	.quick-reply {
		display: flex;
		gap: 8px;
		padding: 6px 0;
	}
	.qr-approve {
		background: var(--success);
		color: #000;
		border: none;
		border-radius: 6px;
		padding: 6px 16px;
		font-size: 0.8rem;
		font-weight: 600;
		cursor: pointer;
	}
	.qr-approve:hover {
		filter: brightness(0.85);
	}
	.qr-reject {
		background: transparent;
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 6px 16px;
		font-size: 0.8rem;
		cursor: pointer;
	}
	.qr-reject:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.injection-warning {
		background: color-mix(in srgb, var(--error) 10%, transparent);
		border-left: 3px solid var(--error);
		padding-left: 8px;
		border-radius: 4px;
	}
	.injection-warning .text {
		color: var(--error) !important;
		font-style: normal !important;
	}
</style>
