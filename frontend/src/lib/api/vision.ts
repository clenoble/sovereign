/** Client for the local jiminy-vision service (gesture + scene + camera feed).
 *
 * The service runs separately (jiminy-vision/vision_service.py) on :9101 and
 * has permissive CORS, so the webview can fetch it directly. The camera frame
 * is shown via an <img> (no CORS needed); state + window control use fetch. */

const VISION_BASE = 'http://127.0.0.1:9101';

export interface VisionStateDto {
	gesture: string | null;
	scene: string;
	window_active: boolean;
	window_remaining_s: number;
	camera_ok: boolean;
}

/** Cache-busted URL for the latest camera JPEG (use as an <img> src). */
export const visionFrameUrl = (cacheBust: number) => `${VISION_BASE}/vision/frame?t=${cacheBust}`;

export async function fetchVisionState(): Promise<VisionStateDto> {
	const r = await fetch(`${VISION_BASE}/vision/state`);
	if (!r.ok) throw new Error(`vision state ${r.status}`);
	return r.json();
}

/** Open the windowed VLM scene-understanding for `durationS` seconds. */
export async function openVisionWindow(durationS?: number): Promise<void> {
	await fetch(`${VISION_BASE}/vision/window`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ duration_s: durationS ?? null })
	});
}

export async function closeVisionWindow(): Promise<void> {
	await fetch(`${VISION_BASE}/vision/window/stop`, { method: 'POST' });
}
