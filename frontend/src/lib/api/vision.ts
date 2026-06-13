/** Client for the local jiminy-vision service (gesture + scene + camera feed).
 *
 * The service runs separately (jiminy-vision/vision_service.py) on :9101. It
 * enforces a bearer token on every request when one is configured, regardless
 * of Origin (SIDECAR-001/002) — a forged allow-listed Origin no longer bypasses
 * auth. So every call here carries `Authorization: Bearer <token>`, where the
 * token comes from the Rust app via the `get_jiminy_token` IPC command. Because
 * an `<img src>` cannot send an Authorization header, the camera frame is
 * fetched as a blob and exposed as an object URL. */
import { invoke } from '@tauri-apps/api/core';

const VISION_BASE = 'http://127.0.0.1:9101';

export interface VisionStateDto {
	gesture: string | null;
	scene: string;
	window_active: boolean;
	window_remaining_s: number;
	camera_ok: boolean;
}

// The sidecar token is provisioned once at app startup and stable for the
// process lifetime, so fetch it lazily and cache it. `undefined` = not yet
// fetched; `null` = no token provisioned (sidecars not in use / dev mode).
let cachedToken: string | null | undefined;

async function authHeaders(): Promise<Record<string, string>> {
	if (cachedToken === undefined) {
		try {
			cachedToken = (await invoke<string | null>('get_jiminy_token')) ?? null;
		} catch {
			cachedToken = null;
		}
	}
	return cachedToken ? { Authorization: `Bearer ${cachedToken}` } : {};
}

export async function fetchVisionState(): Promise<VisionStateDto> {
	const r = await fetch(`${VISION_BASE}/vision/state`, { headers: await authHeaders() });
	if (!r.ok) throw new Error(`vision state ${r.status}`);
	return r.json();
}

/** Fetch the latest camera JPEG WITH the bearer token and return an object URL.
 * The caller owns the URL and must `URL.revokeObjectURL` it once replaced. */
export async function fetchVisionFrameObjectUrl(): Promise<string> {
	const r = await fetch(`${VISION_BASE}/vision/frame`, { headers: await authHeaders() });
	if (!r.ok) throw new Error(`vision frame ${r.status}`);
	return URL.createObjectURL(await r.blob());
}

/** Open the windowed VLM scene-understanding for `durationS` seconds. */
export async function openVisionWindow(durationS?: number): Promise<void> {
	await fetch(`${VISION_BASE}/vision/window`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json', ...(await authHeaders()) },
		body: JSON.stringify({ duration_s: durationS ?? null })
	});
}

export async function closeVisionWindow(): Promise<void> {
	await fetch(`${VISION_BASE}/vision/window/stop`, {
		method: 'POST',
		headers: await authHeaders()
	});
}
