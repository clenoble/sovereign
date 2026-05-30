/** Svelte action — keyboard focus trap for dialogs and floating panels.
 *
 * Step 6e of the PII management & dashboard plan. Establishes the
 * accessibility precedent for new floating panels: when the user opens
 * a panel keyboard-only, focus moves into it; Tab cycles within; Escape
 * closes; the previously-focused element gets focus back on close.
 *
 * Usage:
 *   <div use:focusTrap={{ active, onEscape: () => close() }}>
 *
 * `active=false` makes the trap a no-op so it can be applied to a panel
 * unconditionally and only enforces when the panel is open.
 */

export interface FocusTrapOptions {
	/** When false, the action is inert. Default: true. */
	active?: boolean;
	/** Called when the user presses Escape while focus is inside the
	 *  trapped node. Implementations typically toggle the panel closed. */
	onEscape?: () => void;
}

const FOCUSABLE_SELECTOR = [
	'button:not([disabled])',
	'input:not([disabled]):not([type="hidden"])',
	'select:not([disabled])',
	'textarea:not([disabled])',
	'a[href]',
	'[tabindex]:not([tabindex="-1"])'
].join(', ');

export function focusTrap(node: HTMLElement, options: FocusTrapOptions = {}) {
	let active = options.active ?? true;
	let onEscape = options.onEscape;
	const previouslyFocused = document.activeElement as HTMLElement | null;

	function focusable(): HTMLElement[] {
		return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!active) return;
		if (e.key === 'Escape' && onEscape) {
			e.preventDefault();
			onEscape();
			return;
		}
		if (e.key !== 'Tab') return;

		// Only constrain Tab when the focus is currently inside the
		// trapped node. Outside-focus tab passes through normally.
		if (!node.contains(document.activeElement)) return;

		const els = focusable();
		if (els.length === 0) {
			e.preventDefault();
			return;
		}
		const first = els[0];
		const last = els[els.length - 1];
		if (e.shiftKey && document.activeElement === first) {
			e.preventDefault();
			last.focus();
		} else if (!e.shiftKey && document.activeElement === last) {
			e.preventDefault();
			first.focus();
		}
	}

	function activate() {
		if (!active) return;
		// Defer to next tick so the node's children are mounted before
		// querying focusable elements.
		queueMicrotask(() => {
			const els = focusable();
			if (els.length > 0 && !node.contains(document.activeElement)) {
				els[0].focus();
			}
		});
	}

	document.addEventListener('keydown', handleKeydown);
	activate();

	return {
		update(newOptions: FocusTrapOptions) {
			const wasActive = active;
			if (newOptions.active !== undefined) active = newOptions.active;
			if (newOptions.onEscape !== undefined) onEscape = newOptions.onEscape;
			// Re-focus into the panel on the activation transition.
			if (!wasActive && active) {
				activate();
			}
		},
		destroy() {
			document.removeEventListener('keydown', handleKeydown);
			// Restore focus to whatever had it before the trap mounted.
			// Best-effort: if the element has been removed, the call is a
			// silent no-op.
			previouslyFocused?.focus?.();
		}
	};
}
