import { beforeEach, describe, expect, it } from 'vitest';
import { applyVisionState, setWindowSeconds, vision } from './vision.svelte';

beforeEach(() => {
	vision.open = false;
	vision.cameraOk = false;
	vision.gesture = null;
	vision.scene = '';
	vision.windowActive = false;
	vision.windowRemaining = 0;
});

describe('applyVisionState', () => {
	it('maps a full /vision/state payload onto the store', () => {
		applyVisionState({
			gesture: 'shush',
			scene: 'a person waving',
			window_active: true,
			window_remaining_s: 120.5,
			camera_ok: true
		});
		expect(vision.gesture).toBe('shush');
		expect(vision.scene).toBe('a person waving');
		expect(vision.windowActive).toBe(true);
		expect(vision.windowRemaining).toBe(120.5);
		expect(vision.cameraOk).toBe(true);
	});

	it('defaults missing fields', () => {
		applyVisionState({ camera_ok: true });
		expect(vision.gesture).toBe(null);
		expect(vision.scene).toBe('');
		expect(vision.windowActive).toBe(false);
		expect(vision.windowRemaining).toBe(0);
		expect(vision.cameraOk).toBe(true);
	});

	it('clears a previous gesture/scene when the payload reports none', () => {
		vision.gesture = 'point';
		vision.scene = 'old caption';
		applyVisionState({ gesture: null, scene: '', camera_ok: true });
		expect(vision.gesture).toBe(null);
		expect(vision.scene).toBe('');
	});
});

describe('setWindowSeconds', () => {
	it('clamps to [10, 3600] and updates the store', () => {
		setWindowSeconds(600);
		expect(vision.windowSeconds).toBe(600);
		setWindowSeconds(5);
		expect(vision.windowSeconds).toBe(10);
		setWindowSeconds(99999);
		expect(vision.windowSeconds).toBe(3600);
	});
});
