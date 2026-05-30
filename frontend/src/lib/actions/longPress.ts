/** Svelte action — fire a callback after a sustained press without movement.
 *
 *  Mobile equivalent of desktop right-click. Press-and-hold for `duration` ms
 *  with no more than `moveTolerance` pixels of movement triggers `onLongPress`.
 *  Releasing early or moving past the tolerance cancels.
 *
 *  Usage:
 *    <button use:longPress={{ onLongPress: (e) => openMenu(e) }}>...</button>
 *
 *  Notes:
 *    - Uses pointer events so it works for mouse + touch + pen.
 *    - The callback receives the original `pointerdown` event so consumers
 *      can read clientX/clientY for positioning a menu under the finger.
 *    - Active by default; pass `active: false` to make the action inert.
 */

export interface LongPressOptions {
	onLongPress: (e: PointerEvent) => void;
	/** Hold duration in ms. Default 500. */
	duration?: number;
	/** Max movement (px) tolerated during the hold before cancellation. Default 8. */
	moveTolerance?: number;
	/** When false, the action is inert. Default true. */
	active?: boolean;
}

export function longPress(node: HTMLElement, params: LongPressOptions) {
	let opts = params;
	let timer: number | null = null;
	let startX = 0;
	let startY = 0;
	let startEvent: PointerEvent | null = null;

	function down(e: PointerEvent) {
		if (opts.active === false) return;
		// Only respond to the primary pointer (avoid two-finger gestures
		// firing a long-press by accident).
		if (!e.isPrimary) return;
		startX = e.clientX;
		startY = e.clientY;
		startEvent = e;
		const duration = opts.duration ?? 500;
		timer = window.setTimeout(() => {
			if (startEvent) opts.onLongPress(startEvent);
			timer = null;
		}, duration);
	}

	function move(e: PointerEvent) {
		if (timer === null) return;
		const tolerance = opts.moveTolerance ?? 8;
		if (
			Math.abs(e.clientX - startX) > tolerance ||
			Math.abs(e.clientY - startY) > tolerance
		) {
			cancel();
		}
	}

	function up() {
		cancel();
	}

	function cancel() {
		if (timer !== null) {
			clearTimeout(timer);
			timer = null;
		}
		startEvent = null;
	}

	node.addEventListener('pointerdown', down);
	node.addEventListener('pointermove', move);
	node.addEventListener('pointerup', up);
	node.addEventListener('pointercancel', cancel);
	node.addEventListener('pointerleave', cancel);

	return {
		update(next: LongPressOptions) {
			opts = next;
		},
		destroy() {
			cancel();
			node.removeEventListener('pointerdown', down);
			node.removeEventListener('pointermove', move);
			node.removeEventListener('pointerup', up);
			node.removeEventListener('pointercancel', cancel);
			node.removeEventListener('pointerleave', cancel);
		}
	};
}
