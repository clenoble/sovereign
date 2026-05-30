import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { longPress } from './longPress';

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEl(): HTMLDivElement {
	const el = document.createElement('div');
	document.body.appendChild(el);
	return el;
}

function pd(el: HTMLElement, opts: Partial<PointerEventInit> = {}): PointerEvent {
	const e = new PointerEvent('pointerdown', { bubbles: true, isPrimary: true, ...opts });
	el.dispatchEvent(e);
	return e;
}

function pm(el: HTMLElement, opts: Partial<PointerEventInit> = {}) {
	el.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, isPrimary: true, ...opts }));
}

function pu(el: HTMLElement) {
	el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, isPrimary: true }));
}

function pc(el: HTMLElement) {
	el.dispatchEvent(new PointerEvent('pointercancel', { bubbles: true, isPrimary: true }));
}

function pl(el: HTMLElement) {
	el.dispatchEvent(new PointerEvent('pointerleave', { bubbles: true, isPrimary: true }));
}

// ── Setup / teardown ──────────────────────────────────────────────────────────

beforeEach(() => {
	vi.useFakeTimers();
});

afterEach(() => {
	vi.useRealTimers();
	document.body.innerHTML = '';
});

// ── Fires the callback ────────────────────────────────────────────────────────

describe('fires after the hold duration', () => {
	it('calls onLongPress after the default 500 ms', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el);
		expect(cb).not.toHaveBeenCalled();

		vi.advanceTimersByTime(500);
		expect(cb).toHaveBeenCalledOnce();
	});

	it('does NOT call before the duration elapses', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el);
		vi.advanceTimersByTime(499);
		expect(cb).not.toHaveBeenCalled();

		vi.advanceTimersByTime(1);
		expect(cb).toHaveBeenCalledOnce();
	});

	it('respects a custom duration', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb, duration: 200 });

		pd(el);
		vi.advanceTimersByTime(199);
		expect(cb).not.toHaveBeenCalled();

		vi.advanceTimersByTime(1);
		expect(cb).toHaveBeenCalledOnce();
	});

	it('passes the original pointerdown event to the callback', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el, { clientX: 42, clientY: 99 });
		vi.advanceTimersByTime(500);

		const arg = cb.mock.calls[0][0] as PointerEvent;
		expect(arg).toBeInstanceOf(PointerEvent);
		expect(arg.clientX).toBe(42);
		expect(arg.clientY).toBe(99);
	});

	it('only fires once per press — not repeatedly', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el);
		vi.advanceTimersByTime(2000); // well past the duration
		expect(cb).toHaveBeenCalledOnce();
	});
});

// ── Early release cancels ─────────────────────────────────────────────────────

describe('early release cancels the timer', () => {
	it('does NOT fire when pointer is released before the duration', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el);
		vi.advanceTimersByTime(400);
		pu(el); // released early
		vi.advanceTimersByTime(200); // time continues past 500 ms total

		expect(cb).not.toHaveBeenCalled();
	});

	it('does NOT fire when pointercancel fires mid-hold', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el);
		vi.advanceTimersByTime(300);
		pc(el);
		vi.advanceTimersByTime(300);

		expect(cb).not.toHaveBeenCalled();
	});

	it('does NOT fire when pointerleave fires mid-hold', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el);
		vi.advanceTimersByTime(300);
		pl(el);
		vi.advanceTimersByTime(300);

		expect(cb).not.toHaveBeenCalled();
	});
});

// ── Movement cancels ──────────────────────────────────────────────────────────

describe('movement beyond tolerance cancels', () => {
	it('cancels when horizontal movement exceeds the default 8 px tolerance', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el, { clientX: 0, clientY: 0 });
		pm(el, { clientX: 9, clientY: 0 }); // 9 px > 8 px
		vi.advanceTimersByTime(500);

		expect(cb).not.toHaveBeenCalled();
	});

	it('cancels when vertical movement exceeds the tolerance', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el, { clientX: 0, clientY: 0 });
		pm(el, { clientX: 0, clientY: 9 }); // 9 px > 8 px
		vi.advanceTimersByTime(500);

		expect(cb).not.toHaveBeenCalled();
	});

	it('still fires when movement is at the tolerance boundary (8 px)', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el, { clientX: 0, clientY: 0 });
		pm(el, { clientX: 8, clientY: 0 }); // exactly 8 px — NOT over tolerance (> 8 required to cancel)
		vi.advanceTimersByTime(500);

		expect(cb).toHaveBeenCalledOnce();
	});

	it('still fires when movement is well within the tolerance', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		pd(el, { clientX: 0, clientY: 0 });
		pm(el, { clientX: 3, clientY: 3 }); // diagonal ~4.2 px per axis — both within 8
		vi.advanceTimersByTime(500);

		expect(cb).toHaveBeenCalledOnce();
	});

	it('respects a custom moveTolerance', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb, moveTolerance: 3 });

		pd(el, { clientX: 0, clientY: 0 });
		pm(el, { clientX: 4, clientY: 0 }); // 4 > 3 — cancelled
		vi.advanceTimersByTime(500);

		expect(cb).not.toHaveBeenCalled();
	});

	it('still fires when movement is within a custom tolerance', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb, moveTolerance: 3 });

		pd(el, { clientX: 0, clientY: 0 });
		pm(el, { clientX: 2, clientY: 0 }); // 2 <= 3 — allowed
		vi.advanceTimersByTime(500);

		expect(cb).toHaveBeenCalledOnce();
	});

	it('does not check movement before a pointerdown (stale state guard)', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		// Move without a preceding pointerdown — should be ignored
		pm(el, { clientX: 999, clientY: 999 });
		pd(el, { clientX: 0, clientY: 0 });
		vi.advanceTimersByTime(500);

		expect(cb).toHaveBeenCalledOnce();
	});
});

// ── active flag ───────────────────────────────────────────────────────────────

describe('active: false disables the action', () => {
	it('does not fire when active is false', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb, active: false });

		pd(el);
		vi.advanceTimersByTime(500);

		expect(cb).not.toHaveBeenCalled();
	});

	it('fires after update() sets active to true', () => {
		const cb = vi.fn();
		const el = makeEl();
		const action = longPress(el, { onLongPress: cb, active: false });

		pd(el);
		vi.advanceTimersByTime(500);
		expect(cb).not.toHaveBeenCalled();

		action.update({ onLongPress: cb, active: true });
		pd(el);
		vi.advanceTimersByTime(500);
		expect(cb).toHaveBeenCalledOnce();
	});

	it('stops firing after update() sets active to false', () => {
		const cb = vi.fn();
		const el = makeEl();
		const action = longPress(el, { onLongPress: cb, active: true });

		// First press fires correctly
		pd(el);
		vi.advanceTimersByTime(500);
		expect(cb).toHaveBeenCalledOnce();

		// Disable and confirm it stops
		action.update({ onLongPress: cb, active: false });
		pd(el);
		vi.advanceTimersByTime(500);
		expect(cb).toHaveBeenCalledOnce(); // count unchanged
	});
});

// ── Primary pointer ───────────────────────────────────────────────────────────

describe('primary pointer only', () => {
	it('ignores non-primary pointers (isPrimary = false)', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, isPrimary: false }));
		vi.advanceTimersByTime(500);

		expect(cb).not.toHaveBeenCalled();
	});

	it('fires for the primary pointer after a non-primary was ignored', () => {
		const cb = vi.fn();
		const el = makeEl();
		longPress(el, { onLongPress: cb });

		// Non-primary first — ignored
		el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, isPrimary: false }));
		// Primary second — should fire
		pd(el, { clientX: 0, clientY: 0 });
		vi.advanceTimersByTime(500);

		expect(cb).toHaveBeenCalledOnce();
	});
});

// ── update() callback replacement ────────────────────────────────────────────

describe('update() replaces the callback', () => {
	it('uses the new callback provided via update()', () => {
		const cb1 = vi.fn();
		const cb2 = vi.fn();
		const el = makeEl();
		const action = longPress(el, { onLongPress: cb1 });

		action.update({ onLongPress: cb2 });
		pd(el);
		vi.advanceTimersByTime(500);

		expect(cb1).not.toHaveBeenCalled();
		expect(cb2).toHaveBeenCalledOnce();
	});
});

// ── destroy() ────────────────────────────────────────────────────────────────

describe('destroy()', () => {
	it('prevents the callback from firing after destruction', () => {
		const cb = vi.fn();
		const el = makeEl();
		const action = longPress(el, { onLongPress: cb });

		pd(el);
		vi.advanceTimersByTime(300);
		action.destroy();
		vi.advanceTimersByTime(300); // would have completed the timer

		expect(cb).not.toHaveBeenCalled();
	});

	it('is safe to call destroy() before any pointerdown', () => {
		const el = makeEl();
		const action = longPress(el, { onLongPress: vi.fn() });
		expect(() => action.destroy()).not.toThrow();
	});

	it('is safe to call destroy() twice', () => {
		const el = makeEl();
		const action = longPress(el, { onLongPress: vi.fn() });
		action.destroy();
		expect(() => action.destroy()).not.toThrow();
	});
});
