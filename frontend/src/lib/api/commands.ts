/** Typed wrappers around Tauri invoke() calls. */

import { invoke } from '@tauri-apps/api/core';

export interface AppStatus {
	documents: number;
	threads: number;
	contacts: number;
	orchestrator_available: boolean;
}

export interface DocSummary {
	id: string;
	title: string;
	thread_id: string;
	is_owned: boolean;
	modified_at: string;
}

export interface ThreadSummary {
	id: string;
	name: string;
	description: string;
}

export interface SearchHit {
	id: string;
	title: string;
	snippet: string;
}

// Health / status
export const greet = (name: string) => invoke<string>('greet', { name });
export const getStatus = () => invoke<AppStatus>('get_status');

// Chat
export const chatMessage = (message: string) => invoke<void>('chat_message', { message });

// Search
export const searchDocuments = (query: string) => invoke<SearchHit[]>('search_documents', { query });
export const searchQuery = (query: string) => invoke<void>('search_query', { query });

// Action gate
export const approveAction = () => invoke<void>('approve_action');
export const rejectAction = (reason: string) => invoke<void>('reject_action', { reason });
export const acceptSuggestion = (action: string) => invoke<void>('accept_suggestion', { action });
export const dismissSuggestion = (action: string) => invoke<void>('dismiss_suggestion', { action });

// Documents
export const listDocuments = (threadId?: string) =>
	invoke<DocSummary[]>('list_documents', { threadId: threadId ?? null });
export const listThreads = () => invoke<ThreadSummary[]>('list_threads');

// Theme
export const toggleTheme = () => invoke<string>('toggle_theme');
export const getTheme = () => invoke<string>('get_theme');
