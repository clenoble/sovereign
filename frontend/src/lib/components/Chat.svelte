<script lang="ts">
	import { chat } from '$lib/stores/chat';
	import { pendingAction } from '$lib/stores/app';
	import { chatMessage, approveAction, rejectAction } from '$lib/api/commands';
	import { get } from 'svelte/store';

	let inputValue = $state('');
	let messagesEl: HTMLDivElement | undefined = $state();

	const messages = chat.recent;
	const generating = chat.generating;
	const visible = chat.visible;

	function scrollToBottom() {
		if (messagesEl) {
			messagesEl.scrollTop = messagesEl.scrollHeight;
		}
	}

	// Scroll when new messages arrive
	$effect(() => {
		$messages;
		// Use setTimeout to wait for DOM update
		setTimeout(scrollToBottom, 0);
	});

	async function handleSubmit() {
		const text = inputValue.trim();
		if (!text) return;
		inputValue = '';

		// Check if there's a pending action — handle confirmation via chat
		const pending = get(pendingAction);
		if (pending) {
			const lower = text.toLowerCase();
			if (['yes', 'y', 'go ahead', 'sure', 'ok', 'okay', 'approve'].includes(lower)) {
				chat.pushUser(text);
				chat.pushSystem('Approved.');
				await approveAction();
				return;
			} else if (['no', 'n', 'cancel', 'reject', 'stop'].includes(lower)) {
				chat.pushUser(text);
				chat.pushSystem('Rejected.');
				await rejectAction('User rejected via chat');
				return;
			}
			// Any other input cancels the pending action and processes normally
			await rejectAction('User provided new input');
		}

		chat.pushUser(text);
		try {
			await chatMessage(text);
		} catch (e) {
			chat.pushSystem(`Error: ${e}`);
			chat.clearGenerating();
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
</script>

{#if $visible}
	<div class="chat-panel">
		<div class="chat-header">
			<span class="chat-title">Chat</span>
			<button class="close-btn" onclick={() => chat.toggle()}>X</button>
		</div>

		<div class="messages" bind:this={messagesEl}>
			{#each $messages as msg}
				<div class="message {msg.role}">
					<span class="prefix">{rolePrefix(msg.role)}</span>
					<span class="text">{msg.text}</span>
				</div>
			{/each}

			{#if $generating}
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
		bottom: 60px;
		left: 50%;
		transform: translateX(-50%);
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
</style>
