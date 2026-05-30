import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { device, initDevice, destroyDevice } from './device.svelte';

// ── UA helpers ────────────────────────────────────────────────────────────────
const UA_DESKTOP = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36';
const UA_IPHONE  = 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1';
const UA_IPAD    = 'Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1';
const UA_ANDROID = 'Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36';

function setUA(ua: string) {
	Object.defineProperty(navigator, 'userAgent', { get: () => ua, configurable: true });
}

function setWidth(w: number) {
	Object.defineProperty(window, 'innerWidth', { get: () => w, configurable: true });
}

// ── Reset state before each test ─────────────────────────────────────────────
beforeEach(() => {
	destroyDevice();
	device.viewportWidth = 1280;
	device.viewportHeight = 800;
	device.platform = 'desktop';
	device.isTouch = false;
	device.isMobile = false;
	setWidth(1280);
	setUA(UA_DESKTOP);
});

afterEach(() => {
	destroyDevice();
});

// ── Platform detection ────────────────────────────────────────────────────────
describe('platform detection', () => {
	it('detects "desktop" when the UA contains no mobile keywords', () => {
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.platform).toBe('desktop');
	});

	it('detects "ios" from an iPhone UA', () => {
		setUA(UA_IPHONE);
		initDevice();
		expect(device.platform).toBe('ios');
	});

	it('detects "ios" from an iPad UA', () => {
		setUA(UA_IPAD);
		initDevice();
		expect(device.platform).toBe('ios');
	});

	it('detects "android" from an Android UA', () => {
		setUA(UA_ANDROID);
		initDevice();
		expect(device.platform).toBe('android');
	});

	it('stores the detected platform in device.platform', () => {
		setUA(UA_IPHONE);
		initDevice();
		expect(device.platform).toBe('ios');

		setUA(UA_DESKTOP);
		initDevice();
		expect(device.platform).toBe('desktop');
	});
});

// ── isMobile — viewport breakpoint ────────────────────────────────────────────
describe('isMobile — viewport breakpoint', () => {
	it('is false on a wide (1280px) desktop viewport', () => {
		setWidth(1280);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(false);
	});

	it('is true at exactly the 768 px breakpoint', () => {
		setWidth(768);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(true);
	});

	it('is false at 769 px with a desktop UA', () => {
		setWidth(769);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(false);
	});

	it('is true at a typical phone width (375 px)', () => {
		setWidth(375);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(true);
	});
});

// ── isMobile — platform override ─────────────────────────────────────────────
describe('isMobile — platform UA override', () => {
	it('is true on an iOS UA even with a wide viewport', () => {
		setWidth(1280); // e.g. iPad in landscape with Tauri
		setUA(UA_IPHONE);
		initDevice();
		expect(device.isMobile).toBe(true);
	});

	it('is true on an Android UA even with a wide viewport', () => {
		setWidth(1280);
		setUA(UA_ANDROID);
		initDevice();
		expect(device.isMobile).toBe(true);
	});
});

// ── Reactive resize ───────────────────────────────────────────────────────────
describe('resize reactivity', () => {
	it('flips isMobile to true when the window narrows past the breakpoint', () => {
		setWidth(1280);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(false);

		setWidth(375);
		window.dispatchEvent(new Event('resize'));
		expect(device.isMobile).toBe(true);
	});

	it('flips isMobile back to false when the window widens above the breakpoint', () => {
		setWidth(375);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(true);

		setWidth(1280);
		window.dispatchEvent(new Event('resize'));
		expect(device.isMobile).toBe(false);
	});

	it('updates device.viewportWidth on resize', () => {
		setWidth(1280);
		setUA(UA_DESKTOP);
		initDevice();

		setWidth(500);
		window.dispatchEvent(new Event('resize'));
		expect(device.viewportWidth).toBe(500);
	});
});

// ── destroyDevice ─────────────────────────────────────────────────────────────
describe('destroyDevice', () => {
	it('is safe to call without a prior initDevice', () => {
		expect(() => destroyDevice()).not.toThrow();
	});

	it('stops responding to resize events after being called', () => {
		setWidth(1280);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(false);

		destroyDevice();

		// Width changes and a resize event fire — state must NOT change
		setWidth(375);
		window.dispatchEvent(new Event('resize'));
		expect(device.isMobile).toBe(false);
	});

	it('is safe to call twice in a row', () => {
		initDevice();
		expect(() => { destroyDevice(); destroyDevice(); }).not.toThrow();
	});
});

// ── initDevice — idempotence ──────────────────────────────────────────────────
describe('initDevice — idempotence', () => {
	it('produces consistent state when called twice (second call wins)', () => {
		setWidth(375);
		setUA(UA_DESKTOP);
		initDevice();
		expect(device.isMobile).toBe(true);

		setWidth(1280);
		initDevice(); // second call with wider viewport
		expect(device.isMobile).toBe(false);
	});
});
