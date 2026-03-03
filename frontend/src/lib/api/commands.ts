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

export interface FullDocument {
	id: string;
	title: string;
	body: string;
	images: ContentImageDto[];
	videos: ContentVideoDto[];
	thread_id: string;
	is_owned: boolean;
	created_at: string;
	modified_at: string;
}

export interface ContentImageDto {
	path: string;
	caption: string;
}

export interface ContentVideoDto {
	path: string;
	caption: string;
	duration_secs: number | null;
	thumbnail_path: string | null;
}

export interface CommitSummary {
	id: string;
	message: string;
	timestamp: string;
	snapshot_title: string;
	snapshot_preview: string;
}

export interface SkillInfo {
	skill_name: string;
	actions: SkillActionInfo[];
}

export interface SkillActionInfo {
	action_id: string;
	label: string;
}

export interface SkillResultDto {
	kind: string;
	body?: string;
	images?: ContentImageDto[];
	videos?: ContentVideoDto[];
	file_name?: string;
	file_mime?: string;
	file_data_base64?: string;
	structured_kind?: string;
	structured_json?: string;
}

export interface ModelEntry {
	filename: string;
	size_mb: number;
	is_router: boolean;
	is_reasoning: boolean;
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

// Document CRUD
export const getDocument = (id: string) => invoke<FullDocument>('get_document', { id });
export const saveDocument = (
	id: string,
	title: string,
	body: string,
	images: ContentImageDto[],
	videos: ContentVideoDto[]
) => invoke<void>('save_document', { id, title, body, images, videos });
export const createDocument = (title: string, threadId: string) =>
	invoke<string>('create_document', { title, threadId });
export const closeDocument = (id: string) => invoke<void>('close_document', { id });

// Version history
export const listCommits = (docId: string) => invoke<CommitSummary[]>('list_commits', { docId });
export const restoreCommit = (docId: string, commitId: string) =>
	invoke<FullDocument>('restore_commit', { docId, commitId });

// Skills
export const listSkillsForDoc = (docTitle: string) =>
	invoke<SkillInfo[]>('list_skills_for_doc', { docTitle });
export const executeSkill = (skillName: string, action: string, docId: string, params: string) =>
	invoke<SkillResultDto>('execute_skill', { skillName, action, docId, params });
export const listAllSkills = () => invoke<SkillInfo[]>('list_all_skills');

// Model management
export const scanModels = () => invoke<ModelEntry[]>('scan_models');
export const assignModelRole = (filename: string, role: string) =>
	invoke<void>('assign_model_role', { filename, role });
export const deleteModel = (filename: string) => invoke<void>('delete_model', { filename });

// ---------------------------------------------------------------------------
// Phase 3: Canvas, Threads, Contacts, Messaging
// ---------------------------------------------------------------------------

export interface CanvasDocDto {
	id: string;
	title: string;
	thread_id: string;
	is_owned: boolean;
	spatial_x: number;
	spatial_y: number;
	created_at: string;
	modified_at: string;
}

export interface ThreadDto {
	id: string;
	name: string;
	description: string;
	created_at: string;
}

export interface RelationshipDto {
	id: string;
	from_doc_id: string;
	to_doc_id: string;
	relation_type: string;
	strength: number;
}

export interface ContactSummaryDto {
	id: string;
	name: string;
	avatar: string | null;
	unread_count: number;
	channels: string[];
}

export interface ContactDetailDto {
	id: string;
	name: string;
	avatar: string | null;
	notes: string;
	addresses: ChannelAddressDto[];
	conversations: ConversationDto[];
}

export interface ChannelAddressDto {
	channel: string;
	address: string;
	display_name: string | null;
	is_primary: boolean;
}

export interface ConversationDto {
	id: string;
	title: string;
	channel: string;
	participant_ids: string[];
	unread_count: number;
	last_message_at: string | null;
}

export interface MessageDto {
	id: string;
	conversation_id: string;
	direction: string;
	from_contact_id: string;
	subject: string | null;
	body: string;
	sent_at: string;
	read_status: string;
}

export interface MilestoneDto {
	id: string;
	title: string;
	timestamp: string;
	thread_id: string;
	description: string;
}

export interface CanvasMessageDto {
	id: string;
	conversation_id: string;
	thread_id: string;
	contact_id: string;
	subject: string;
	is_outbound: boolean;
	sent_at: string;
}

export interface CanvasData {
	documents: CanvasDocDto[];
	threads: ThreadDto[];
	relationships: RelationshipDto[];
	contacts: ContactSummaryDto[];
	milestones: MilestoneDto[];
	messages: CanvasMessageDto[];
}

// Canvas
export const canvasLoad = () => invoke<CanvasData>('canvas_load');
export const updateDocumentPosition = (id: string, x: number, y: number) =>
	invoke<void>('update_document_position', { id, x, y });

// Thread CRUD
export const createThread = (name: string, description: string) =>
	invoke<ThreadDto>('create_thread', { name, description });
export const updateThread = (id: string, name?: string, description?: string) =>
	invoke<ThreadDto>('update_thread', { id, name: name ?? null, description: description ?? null });
export const deleteThread = (id: string) => invoke<void>('delete_thread', { id });
export const moveDocumentToThread = (docId: string, threadId: string) =>
	invoke<void>('move_document_to_thread', { docId, threadId });

// Contacts & messaging
export const listContacts = () => invoke<ContactSummaryDto[]>('list_contacts');
export const getContactDetail = (id: string) => invoke<ContactDetailDto>('get_contact_detail', { id });
export const listConversations = (contactId?: string) =>
	invoke<ConversationDto[]>('list_conversations', { contactId: contactId ?? null });
export const listMessages = (conversationId: string, before?: string, limit: number = 50) =>
	invoke<MessageDto[]>('list_messages', { conversationId, before: before ?? null, limit });
export const markMessageRead = (id: string) => invoke<void>('mark_message_read', { id });
export const createRelationship = (fromId: string, toId: string, relationType: string, strength: number) =>
	invoke<void>('create_relationship', { fromId, toId, relationType, strength });

// ---------------------------------------------------------------------------
// Phase 4: Auth, Onboarding, Settings, Document deletion
// ---------------------------------------------------------------------------

export interface AuthCheckResult {
	needs_onboarding: boolean;
	needs_login: boolean;
	crypto_enabled: boolean;
}

export interface PasswordValidationDto {
	valid: boolean;
	errors: string[];
}

export interface UserProfileDto {
	user_id: string;
	designation: string;
	nickname: string | null;
	bubble_style: string;
	display_name: string | null;
}

export interface SaveProfileDto {
	nickname?: string;
	bubble_style?: string;
	display_name?: string;
}

export interface AppConfigDto {
	ai_model_dir: string;
	ai_router_model: string;
	ai_reasoning_model: string;
	ai_n_gpu_layers: number;
	ai_n_ctx: number;
	ai_prompt_format: string;
	crypto_enabled: boolean;
	crypto_keystroke_enabled: boolean;
	crypto_max_login_attempts: number;
	crypto_lockout_seconds: number;
	ui_theme: string;
}

export interface KeystrokeSampleDto {
	key: string;
	press_ms: number;
	release_ms: number;
}

export interface OnboardingData {
	nickname: string | null;
	bubble_style: string | null;
	seed_sample_data: boolean;
	password: string | null;
	duress_password: string | null;
	canary_phrase: string | null;
	keystrokes: KeystrokeSampleDto[][];
}

// Auth
export const checkAuthState = () => invoke<AuthCheckResult>('check_auth_state');
export const validatePassword = (password: string, keystrokes: KeystrokeSampleDto[]) =>
	invoke<string>('validate_password', { password, keystrokes });
export const validatePasswordPolicy = (password: string) =>
	invoke<PasswordValidationDto>('validate_password_policy', { password });

// Onboarding
export const completeOnboarding = (data: OnboardingData) =>
	invoke<void>('complete_onboarding', { data });

// Profile
export const getProfile = () => invoke<UserProfileDto>('get_profile');
export const saveProfile = (data: SaveProfileDto) =>
	invoke<void>('save_profile', { data });

// Config
export const getConfig = () => invoke<AppConfigDto>('get_config');

// Document deletion
export const deleteDocument = (id: string) => invoke<void>('delete_document', { id });

// ---------------------------------------------------------------------------
// Phase 5: Trust, Import, Comms
// ---------------------------------------------------------------------------

export interface TrustEntryDto {
	action: string;
	approval_count: number;
	auto_approve: boolean;
	last_rejected: string | null;
}

export interface CommsConfigDto {
	comms_available: boolean;
	email_configured: boolean;
	email_imap_host: string;
	email_imap_port: number;
	email_smtp_host: string;
	email_smtp_port: number;
	email_username: string;
	signal_configured: boolean;
	signal_phone: string;
}

export interface SaveCommsConfigDto {
	email_imap_host?: string;
	email_imap_port?: number;
	email_smtp_host?: string;
	email_smtp_port?: number;
	email_username?: string;
	signal_phone?: string;
}

// Trust dashboard
export const getTrustEntries = () => invoke<TrustEntryDto[]>('get_trust_entries');
export const resetTrustAction = (action: string) =>
	invoke<void>('reset_trust_action', { action });
export const resetTrustAll = () => invoke<void>('reset_trust_all');

// File import
export const importFile = (filePath: string, threadId?: string) =>
	invoke<CanvasDocDto>('import_file', { filePath, threadId: threadId ?? null });

// Comms config
export const getCommsConfig = () => invoke<CommsConfigDto>('get_comms_config');
export const saveCommsConfig = (data: SaveCommsConfigDto) =>
	invoke<void>('save_comms_config', { data });
