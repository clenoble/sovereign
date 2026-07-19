/** Typed wrappers around the guardian app's Tauri commands. */

import { invoke } from '@tauri-apps/api/core';

export interface Duty {
	owner_label: string;
	owner_tag: string;
	shard_id: string;
	epoch: number;
	threshold: number;
	total: number;
	enrolled_at: string;
}

export interface GuardianStatus {
	peer_id: string;
	duties: Duty[];
	pending_count: number;
}

export interface RecoveryRequest {
	/** The friend recovering (owner tag) — the approve/deny key. */
	for_user: string;
	epoch: number;
	request_id: string | null;
	requested_at: string | null;
	shard_id: string;
}

export const guardianStatus = () => invoke<GuardianStatus>('guardian_status');

export const listRecoveryRequests = () =>
	invoke<RecoveryRequest[]>('list_recovery_requests');

export const approveRecovery = (forUser: string, epoch: number) =>
	invoke<boolean>('approve_recovery', { forUser, epoch });

export const denyRecovery = (forUser: string, epoch: number) =>
	invoke<boolean>('deny_recovery', { forUser, epoch });

export const enrollGuardian = (offer: string, code: string, label: string) =>
	invoke<Duty>('enroll_guardian', { offer, code, label });
