/** Vision panel state — Svelte 5 rune store.
 *
 * Reflects the jiminy-vision service: the live gesture, the windowed VLM scene
 * caption, and whether the scene-understanding window is open. The panel polls
 * GET /vision/state once a second while it is visible. */
import { fetchVisionState, type VisionStateDto } from '$lib/api/vision';

const WINDOW_SECONDS_KEY = 'jiminy.vision.windowSeconds';

function loadWindowSeconds(): number {
	try {
		if (typeof localStorage !== 'undefined') {
			const v = Number(localStorage.getItem(WINDOW_SECONDS_KEY));
			if (v >= 10) return v;
		}
	} catch {
		/* ignore */
	}
	return 300;
}

export const vision = $state({
	open: false,
	cameraOk: false,
	gesture: null as string | null,
	scene: '',
	windowActive: false,
	windowRemaining: 0,
	/** How long a "Look" (windowed VLM) runs, in seconds (persisted). */
	windowSeconds: loadWindowSeconds()
});

/** Set the VLM window duration (clamped to [10, 3600]s) and persist it. */
export function setWindowSeconds(seconds: number) {
	const clamped = Math.min(3600, Math.max(10, Math.round(seconds)));
	vision.windowSeconds = clamped;
	try {
		if (typeof localStorage !== 'undefined') {
			localStorage.setItem(WINDOW_SECONDS_KEY, String(clamped));
		}
	} catch {
		/* ignore */
	}
}

/** Apply a polled /vision/state payload onto the store. */
export function applyVisionState(s: Partial<VisionStateDto>) {
	vision.cameraOk = !!s.camera_ok;
	vision.gesture = s.gesture ?? null;
	vision.scene = s.scene ?? '';
	vision.windowActive = !!s.window_active;
	vision.windowRemaining = s.window_remaining_s ?? 0;
}

let pollTimer: ReturnType<typeof setInterval> | null = null;

function stopPolling() {
	if (pollTimer !== null) {
		clearInterval(pollTimer);
		pollTimer = null;
	}
}

function startPolling() {
	stopPolling();
	const tick = async () => {
		try {
			applyVisionState(await fetchVisionState());
		} catch {
			vision.cameraOk = false; // service not running
		}
	};
	tick();
	pollTimer = setInterval(tick, 1000);
}

export function openVisionPanel() {
	vision.open = true;
	startPolling();
}

export function closeVisionPanel() {
	vision.open = false;
	stopPolling();
}

export function toggleVisionPanel() {
	if (vision.open) closeVisionPanel();
	else openVisionPanel();
}
