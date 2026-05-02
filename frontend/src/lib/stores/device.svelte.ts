/** Rune-based reactive state for device + viewport detection.
 *
 *  This is the single source of truth for "are we in mobile mode?" — used
 *  to decide whether to mount MobileShell or the desktop overlay tree.
 *
 *  Mobile mode is triggered by either:
 *    - Viewport <= MOBILE_BREAKPOINT_PX (so resizing a desktop browser
 *      previews the mobile UI), OR
 *    - userAgent indicates iOS/Android (so a Tauri mobile build is
 *      always mobile regardless of viewport).
 */

export type Platform = 'ios' | 'android' | 'desktop';

const MOBILE_BREAKPOINT_PX = 768;

/** Reactive device state. Initialized to safe SSR-friendly defaults; call
 *  initDevice() once on mount in the root layout to read the actual
 *  viewport and subscribe to resize. */
export const device = $state({
	viewportWidth: 1280,
	viewportHeight: 800,
	platform: 'desktop' as Platform,
	isTouch: false,
	/** True when viewport is narrow OR running on iOS/Android. */
	isMobile: false
});

let resizeListener: (() => void) | null = null;

/** Initialize device detection. Idempotent. Safe to call from onMount. */
export function initDevice() {
	if (typeof window === 'undefined') return;

	const ua = navigator.userAgent || '';
	if (/iPhone|iPad|iPod/i.test(ua)) {
		device.platform = 'ios';
	} else if (/Android/i.test(ua)) {
		device.platform = 'android';
	} else {
		device.platform = 'desktop';
	}

	device.isTouch = 'ontouchstart' in window || navigator.maxTouchPoints > 0;

	const recompute = () => {
		device.viewportWidth = window.innerWidth;
		device.viewportHeight = window.innerHeight;
		device.isMobile =
			device.viewportWidth <= MOBILE_BREAKPOINT_PX ||
			device.platform === 'ios' ||
			device.platform === 'android';
	};

	recompute();

	if (resizeListener) {
		window.removeEventListener('resize', resizeListener);
	}
	resizeListener = recompute;
	window.addEventListener('resize', resizeListener);
}

/** Stop listening for viewport changes. Call in onDestroy or layout cleanup. */
export function destroyDevice() {
	if (resizeListener && typeof window !== 'undefined') {
		window.removeEventListener('resize', resizeListener);
	}
	resizeListener = null;
}
