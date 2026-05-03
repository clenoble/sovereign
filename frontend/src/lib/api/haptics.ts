/** Haptic feedback tied to action gravity levels.
 *
 *  Gravity → feedback mapping:
 *    Observe  → nothing
 *    Suggest  → selection (light tick)
 *    Compose  → medium impact
 *    Modify   → heavy impact
 *    Destruct → error notification (distinct double-buzz on iOS)
 *
 *  All functions are no-ops on desktop — the Tauri plugin stubs return
 *  Ok(()) on non-mobile targets so we never need a device guard here.
 */

import {
	impactFeedback,
	notificationFeedback,
	selectionFeedback
} from '@tauri-apps/plugin-haptics';

export async function hapticForLevel(level: string | undefined): Promise<void> {
	try {
		switch (level) {
			case 'Observe':
				break;
			case 'Suggest':
				await selectionFeedback();
				break;
			case 'Compose':
				await impactFeedback('medium');
				break;
			case 'Modify':
				await impactFeedback('heavy');
				break;
			case 'Destruct':
				await notificationFeedback('error');
				break;
		}
	} catch {
		// silently ignore — desktop or permission not granted
	}
}

export async function hapticSuccess(): Promise<void> {
	try {
		await notificationFeedback('success');
	} catch {
		/* no-op */
	}
}

export async function hapticLight(): Promise<void> {
	try {
		await impactFeedback('light');
	} catch {
		/* no-op */
	}
}
