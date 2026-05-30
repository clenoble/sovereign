<script lang="ts">
	import { onMount } from 'svelte';
	import {
		canvas,
		load as canvasLoad,
		refresh as canvasRefresh,
		panBy,
		zoomAt,
		home,
		getVisibleDocuments,
		requestMessagesForViewport,
		CARD_W,
		CARD_H,
		LANE_HEIGHT,
		MSG_RADIUS,
		MAX_VISUAL_ZOOM,
		type CanvasState
	} from '$lib/stores/canvas.svelte';
	import { createThread as apiCreateThread, importFile } from '$lib/api/commands';
	import { app } from '$lib/stores/app.svelte';
	import CanvasCard from './CanvasCard.svelte';
	import Minimap from './Minimap.svelte';

	let canvasEl: HTMLCanvasElement;
	let containerEl: HTMLDivElement;
	let ctx: CanvasRenderingContext2D | null = null;

	// Pan state
	let panning = false;
	let panStart = { x: 0, y: 0 };
	let panCameraStart = { x: 0, y: 0 };

	// Viewport-culled documents (only mount DOM cards for visible docs)
	let visibleDocs = $derived(getVisibleDocuments());

	// Load messages for the visible time range when camera moves
	$effect(() => {
		// Track camera state to re-run on pan/zoom
		void canvas.camera.panX;
		void canvas.camera.panY;
		void canvas.camera.zoom;
		if (canvas.loaded && canvas.timelineScale) {
			requestMessagesForViewport();
		}
	});

	// File drag-and-drop
	let dragOver = $state(false);

	// Thread creation
	let showNewThread = $state(false);
	let newThreadName = $state('');

	onMount(() => {
		canvasLoad();
		ctx = canvasEl.getContext('2d');
		resizeCanvas();
		const resizeObs = new ResizeObserver(resizeCanvas);
		resizeObs.observe(containerEl);
		return () => resizeObs.disconnect();
	});

	function resizeCanvas() {
		if (!canvasEl || !containerEl) return;
		canvasEl.width = containerEl.clientWidth;
		canvasEl.height = containerEl.clientHeight;
		drawBackground(canvas);
	}

	// Redraw background whenever canvas state changes.
	// Explicitly read positions + timeline nowX to ensure Svelte tracks them.
	$effect(() => {
		void canvas.documents.map(d => d.spatial_x + d.spatial_y);
		void canvas.messages.length;
		void canvas.timelineScale?.nowX;
		drawBackground(canvas);
	});

	/** Read a CSS custom property from the canvas container. */
	function getCSS(prop: string): string {
		if (!containerEl) return '';
		return getComputedStyle(containerEl).getPropertyValue(prop).trim();
	}

	function drawBackground(state: CanvasState) {
		if (!ctx || !canvasEl) return;
		const { camera, threads, documents, relationships, milestones, messages, timelineScale } = state;
		const w = canvasEl.width;
		const h = canvasEl.height;
		ctx.clearRect(0, 0, w, h);
		ctx.save();
		ctx.translate(camera.panX, camera.panY);
		ctx.scale(camera.zoom, camera.zoom);

		// Theme-aware colors from CSS custom properties
		const textMuted = getCSS('--text-muted') || '#9a9a9a';
		const textPrimary = getCSS('--text-primary') || '#e0e0e0';
		const warningColor = getCSS('--warning') || '#F59E0B';
		const borderColor = getCSS('--border') || '#333340';
		const accentColor = getCSS('--accent') || '#F59E0B';

		// Draw thread lane backgrounds
		const laneHeight = LANE_HEIGHT;
		const threadOrder = new Map<string, number>();
		threads.forEach((t, i) => threadOrder.set(t.id, i));

		// Find x-extent of documents and messages
		let maxX = 1000;
		for (const d of documents) {
			maxX = Math.max(maxX, d.spatial_x + 220);
		}
		for (const m of messages) {
			maxX = Math.max(maxX, m.x + MSG_RADIUS + 20);
		}
		if (timelineScale) {
			maxX = Math.max(maxX, timelineScale.originX + (timelineScale.maxDate - timelineScale.minDate) * timelineScale.pxPerMs + 100);
		}

		const totalHeight = threads.length * laneHeight;

		// Collected during the world-space pass; rendered as sticky-top labels
		// in screen space after ctx.restore() so they stay visible regardless
		// of pan/zoom in Y.
		const tickLabels: Array<{ worldX: number; text: string }> = [];
		let nowWorldX: number | null = null;

		// -- Timeline date markers along X-axis --
		if (timelineScale) {
			const { minDate, maxDate, pxPerMs, originX } = timelineScale;
			const MS_PER_MIN  = 60_000;
			const MS_PER_HOUR = 3_600_000;
			const MS_PER_DAY  = 86_400_000;
			const pxPerDay = pxPerMs * MS_PER_DAY;
			// effectivePxPerDay: how many screen pixels one day occupies at current zoom
			const effectivePxPerDay = pxPerDay * camera.zoom;
			const MONTHS = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
			const DAYS   = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat'];

			// Choose tick interval based on screen pixel density per day
			let intervalMs: number;
			let formatTick: (d: Date) => string;
			let alignToMonth = false;
			let alignToYear  = false;
			let alignToHour  = false;  // also used for minute-level intervals

			if (effectivePxPerDay > 2000) {
				// Every 10 minutes: "14:30"
				intervalMs = MS_PER_MIN * 10;
				alignToHour = true;
				formatTick = (d) =>
					`${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
			} else if (effectivePxPerDay > 600) {
				// Every hour: "14:00"
				intervalMs = MS_PER_HOUR;
				alignToHour = true;
				formatTick = (d) => `${String(d.getHours()).padStart(2, '0')}:00`;
			} else if (effectivePxPerDay > 200) {
				// Every 6 hours: "Mon 14:00"
				intervalMs = MS_PER_HOUR * 6;
				alignToHour = true;
				formatTick = (d) =>
					`${DAYS[d.getDay()]} ${String(d.getHours()).padStart(2, '0')}:00`;
			} else if (effectivePxPerDay > 80) {
				// Daily (default at zoom=1): "3 Mar"
				intervalMs = MS_PER_DAY;
				formatTick = (d) => `${d.getDate()} ${MONTHS[d.getMonth()]}`;
			} else if (effectivePxPerDay > 20) {
				// Weekly: "3/3"
				intervalMs = MS_PER_DAY * 7;
				formatTick = (d) => `${d.getDate()}/${d.getMonth() + 1}`;
			} else if (effectivePxPerDay > 5) {
				// Monthly: "Mar 2026"
				intervalMs = MS_PER_DAY * 30;
				alignToMonth = true;
				formatTick = (d) => `${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
			} else if (effectivePxPerDay > 0.5) {
				// Yearly: "2026"
				intervalMs = MS_PER_DAY * 365;
				alignToYear = true;
				formatTick = (d) => `${d.getFullYear()}`;
			} else {
				// 5-year: "2025"
				intervalMs = MS_PER_DAY * 365 * 5;
				alignToYear = true;
				formatTick = (d) => `${d.getFullYear()}`;
			}

			// Labels are rendered in screen space after ctx.restore() so they
			// stick to the top of the viewport regardless of pan/zoom in Y.
			// Here we just compute tick positions, push label text into the
			// collector, and draw the vertical grid lines in world space.

			// Start from an aligned boundary just before the visible viewport.
			// We fast-forward to the viewport to avoid iterating over thousands of
			// off-screen ticks when zoomed in on a narrow time window.
			const visMinX = -camera.panX / camera.zoom;
			const visMaxX = (w - camera.panX) / camera.zoom;
			const visMinTime = minDate + (visMinX - originX) / pxPerMs;
			const visMaxTime = minDate + (visMaxX - originX) / pxPerMs;

			const startD = new Date(Math.max(minDate, visMinTime - intervalMs));
			if (alignToYear) {
				startD.setMonth(0, 1);
				startD.setHours(0, 0, 0, 0);
			} else if (alignToMonth) {
				startD.setDate(1);
				startD.setHours(0, 0, 0, 0);
			} else if (alignToHour) {
				if (intervalMs <= MS_PER_MIN * 10) {
					const m = startD.getMinutes();
					startD.setMinutes(Math.floor(m / 10) * 10, 0, 0);
				} else if (intervalMs <= MS_PER_HOUR) {
					startD.setMinutes(0, 0, 0);
				} else {
					// 6-hour: snap to nearest 6h boundary (0, 6, 12, 18)
					const h = startD.getHours();
					startD.setHours(Math.floor(h / 6) * 6, 0, 0, 0);
				}
			} else {
				startD.setHours(0, 0, 0, 0);
			}

			// For fixed-interval ticks: fast-forward to just before visible range
			let tick = startD.getTime();
			if (!alignToMonth && !alignToYear && tick < visMinTime - intervalMs) {
				const n = Math.floor((visMinTime - tick) / intervalMs);
				tick += n * intervalMs;
			}

			const tickEnd = Math.min(maxDate, visMaxTime + intervalMs);
			while (tick <= tickEnd) {
				const x = originX + (tick - minDate) * pxPerMs;
				// Defer label rendering to the screen-space pass so the labels
				// stay sticky at the top of the viewport.
				tickLabels.push({ worldX: x, text: formatTick(new Date(tick)) });

				// Vertical grid line — 1px in screen space regardless of zoom
				ctx.strokeStyle = borderColor;
				ctx.globalAlpha = 0.15;
				ctx.lineWidth = 1 / camera.zoom;
				ctx.beginPath();
				ctx.moveTo(x, 0);
				ctx.lineTo(x, totalHeight);
				ctx.stroke();
				ctx.globalAlpha = 1.0;

				// Advance tick
				if (alignToYear) {
					const dt = new Date(tick);
					dt.setFullYear(dt.getFullYear() + (intervalMs > MS_PER_DAY * 365 * 2 ? 5 : 1));
					tick = dt.getTime();
				} else if (alignToMonth) {
					const dt = new Date(tick);
					dt.setMonth(dt.getMonth() + 1);
					tick = dt.getTime();
				} else {
					tick += intervalMs;
				}
			}
		}

		// -- Thread lanes --
		for (let i = 0; i < threads.length; i++) {
			const y = i * laneHeight;
			ctx.fillStyle = i % 2 === 0 ? 'rgba(128,128,128,0.03)' : 'rgba(128,128,128,0.06)';
			ctx.fillRect(-100, y, maxX + 200, laneHeight);

			ctx.strokeStyle = borderColor;
			ctx.globalAlpha = 0.3;
			ctx.lineWidth = 1;
			ctx.beginPath();
			ctx.moveTo(-100, y + laneHeight);
			ctx.lineTo(maxX + 200, y + laneHeight);
			ctx.stroke();
			ctx.globalAlpha = 1.0;
		}

		// -- "Now" dotted vertical line --
		if (timelineScale && threads.length > 0) {
			const nowX = timelineScale.nowX;
			nowWorldX = nowX;
			ctx.save();
			ctx.setLineDash([6 / camera.zoom, 4 / camera.zoom]);
			ctx.strokeStyle = accentColor;
			ctx.lineWidth = 2 / camera.zoom;
			ctx.globalAlpha = 0.7;
			ctx.beginPath();
			ctx.moveTo(nowX, 0);
			ctx.lineTo(nowX, totalHeight + 10);
			ctx.stroke();
			ctx.setLineDash([]);
			ctx.restore();
			// "Now" label is drawn in screen space below (sticky at top).
		}

		// -- Heatmap density bands (extreme zoom-out) --
		if (camera.zoom < 0.15 && timelineScale && documents.length > 0) {
			const { minDate, pxPerMs, originX } = timelineScale;
			const MS_PER_DAY = 86_400_000;
			// Bucket size: 30 days at very low zoom, 7 days at moderate zoom-out
			const bucketMs = camera.zoom < 0.05 ? MS_PER_DAY * 30 : MS_PER_DAY * 7;
			const bucketPx = bucketMs * pxPerMs;
			const provOwned = getCSS('--prov-owned') || '#5a9fd4';
			const provExternal = getCSS('--prov-external') || '#e07c6a';

			// Build density: Map<"laneIdx:bucketIdx", { owned: number, external: number }>
			const density = new Map<string, { owned: number; external: number }>();
			let maxCount = 1;
			const threadOrder2 = new Map<string, number>();
			threads.forEach((t, i) => threadOrder2.set(t.id, i));

			for (const d of documents) {
				const t = new Date(d.modified_at).getTime();
				const bi = Math.floor((t - minDate) / bucketMs);
				const li = threadOrder2.get(d.thread_id) ?? 0;
				const key = `${li}:${bi}`;
				const entry = density.get(key) || { owned: 0, external: 0 };
				if (d.is_owned) entry.owned++; else entry.external++;
				density.set(key, entry);
				maxCount = Math.max(maxCount, entry.owned + entry.external);
			}

			for (const [key, counts] of density) {
				const [li, bi] = key.split(':').map(Number);
				const x = originX + bi * bucketPx;
				const y = li * laneHeight + 4;
				const h = laneHeight - 8;
				const total = counts.owned + counts.external;
				const alpha = 0.1 + 0.7 * (total / maxCount);

				// Owned portion
				if (counts.owned > 0) {
					ctx.fillStyle = provOwned;
					ctx.globalAlpha = alpha;
					const ownedW = bucketPx * (counts.owned / total);
					ctx.fillRect(x, y, ownedW, h);
				}
				// External portion
				if (counts.external > 0) {
					ctx.fillStyle = provExternal;
					ctx.globalAlpha = alpha;
					const ownedW = bucketPx * (counts.owned / total);
					ctx.fillRect(x + ownedW, y, bucketPx - ownedW, h);
				}
			}
			ctx.globalAlpha = 1.0;
		}

		// -- Relationship edges --
		// Cards apply an inverse scale once zoom > MAX_VISUAL_ZOOM, so their
		// visual centers are no longer at (spatial_x + CARD_W/2). We anchor
		// the relationship line to the same scaled center and shrink the
		// curve "lift" + line width by the same factor so they keep matching
		// the visual card sizes at extreme zoom.
		const relCardScale =
			camera.zoom > MAX_VISUAL_ZOOM ? MAX_VISUAL_ZOOM / camera.zoom : 1;
		const cardCenterOffsetX = (CARD_W / 2) * relCardScale;
		const cardCenterOffsetY = (CARD_H / 2) * relCardScale;
		for (const rel of relationships) {
			const fromDoc = documents.find((d) => d.id === rel.from_doc_id);
			const toDoc = documents.find((d) => d.id === rel.to_doc_id);
			if (!fromDoc || !toDoc) continue;

			const fromX = fromDoc.spatial_x + cardCenterOffsetX;
			const fromY = fromDoc.spatial_y + cardCenterOffsetY;
			const toX = toDoc.spatial_x + cardCenterOffsetX;
			const toY = toDoc.spatial_y + cardCenterOffsetY;

			let color = 'rgba(100,180,255,0.65)';
			if (rel.relation_type === 'DerivedFrom') color = 'rgba(255,200,100,0.65)';
			else if (rel.relation_type === 'Contradicts') color = 'rgba(255,100,100,0.65)';
			else if (rel.relation_type === 'Supports') color = 'rgba(100,255,100,0.65)';

			ctx.strokeStyle = color;
			ctx.lineWidth = (1 + rel.strength * 2) * relCardScale;
			ctx.beginPath();
			const midX = (fromX + toX) / 2;
			const midY = (fromY + toY) / 2 - 30 * relCardScale;
			ctx.moveTo(fromX, fromY);
			ctx.quadraticCurveTo(midX, midY, toX, toY);
			ctx.stroke();
		}

		// -- Milestone markers (positioned on timeline) --
		for (const ms of milestones) {
			const thread = threads.find((t) => t.id === ms.thread_id);
			if (!thread) continue;
			const laneIdx = threadOrder.get(ms.thread_id) ?? 0;
			const y = laneIdx * laneHeight;

			let x: number;
			if (timelineScale) {
				const msTime = new Date(ms.timestamp).getTime();
				x = timelineScale.originX + (msTime - timelineScale.minDate) * timelineScale.pxPerMs;
			} else {
				const msTime = new Date(ms.timestamp).getTime();
				x = 200 + ((msTime % 100000000) / 100000000) * maxX;
			}

			ctx.fillStyle = warningColor;
			ctx.globalAlpha = 0.7;
			ctx.beginPath();
			ctx.moveTo(x, y + 5);
			ctx.lineTo(x + 6, y + 15);
			ctx.lineTo(x - 6, y + 15);
			ctx.closePath();
			ctx.fill();
			ctx.globalAlpha = 1.0;

			ctx.fillStyle = warningColor;
			// Inverse-zoom font so milestone labels stay 10px on screen.
			ctx.font = `${10 / camera.zoom}px -apple-system, sans-serif`;
			ctx.fillText(ms.title, x + 8 / camera.zoom, y + 14);
		}

		// -- Message circles --
		// Cap the on-screen radius once zoom exceeds MAX_VISUAL_ZOOM so circles
		// don't grow past their natural size — same approach as CanvasCard.
		const radiusFactor = camera.zoom > MAX_VISUAL_ZOOM ? MAX_VISUAL_ZOOM / camera.zoom : 1;
		const r = MSG_RADIUS * radiusFactor;
		for (const msg of messages) {
			const fillColor = msg.is_outbound ? '#263a1e' : '#2e2433';
			const msgBorderColor = msg.is_outbound ? '#72bf80' : '#a473cc';

			if (camera.zoom < 0.3) {
				// Tiny dot
				ctx.fillStyle = msgBorderColor;
				ctx.beginPath();
				ctx.arc(msg.x, msg.y, 4, 0, Math.PI * 2);
				ctx.fill();
			} else {
				// Filled circle with border. Border width tracks the radius
				// cap so it stops growing past zoom = MAX_VISUAL_ZOOM in the
				// same way the bubble does.
				ctx.fillStyle = fillColor;
				ctx.beginPath();
				ctx.arc(msg.x, msg.y, r, 0, Math.PI * 2);
				ctx.fill();
				ctx.strokeStyle = msgBorderColor;
				ctx.lineWidth = 2 * radiusFactor;
				ctx.stroke();

				// Subject text (truncated). Inverse-zoom so it stays 9px on screen.
				ctx.fillStyle = textPrimary;
				ctx.font = `${9 / camera.zoom}px -apple-system, sans-serif`;
				ctx.textAlign = 'center';
				ctx.textBaseline = 'middle';
				const label = msg.subject.length > 12 ? msg.subject.slice(0, 11) + '\u2026' : msg.subject;
				ctx.fillText(label, msg.x, msg.y);

				if (camera.zoom >= 0.6) {
					// "in" / "out" badge below circle \u2014 same inverse-zoom treatment.
					const badge = msg.is_outbound ? 'out' : 'in';
					ctx.fillStyle = msgBorderColor;
					ctx.font = `bold ${8 / camera.zoom}px -apple-system, sans-serif`;
					ctx.fillText(badge, msg.x, msg.y + r + 10 / camera.zoom);
				}

				ctx.textAlign = 'start';
			}
		}

		ctx.restore();

		// -- Sticky date ticks (screen-space, fixed at top of canvas) --
		// A semi-transparent strip behind the ticks keeps them legible when
		// content scrolls underneath. Drawn after ctx.restore() so the strip
		// and labels stay glued to the viewport regardless of pan/zoom.
		if (timelineScale && tickLabels.length > 0) {
			const stripH = 24;
			const bgPanel = getCSS('--bg-panel') || '#1a1a24';
			ctx.fillStyle = bgPanel;
			ctx.globalAlpha = 0.85;
			ctx.fillRect(0, 0, w, stripH);
			ctx.globalAlpha = 1.0;

			ctx.fillStyle = textMuted;
			ctx.font = '10px -apple-system, sans-serif';
			ctx.textAlign = 'center';
			ctx.textBaseline = 'middle';
			for (const t of tickLabels) {
				const screenX = camera.panX + t.worldX * camera.zoom;
				if (screenX < -40 || screenX > w + 40) continue;
				ctx.fillText(t.text, screenX, stripH / 2);
			}

			// "Now" marker — same strip, accent color, just below the date.
			if (nowWorldX !== null) {
				const nowScreenX = camera.panX + nowWorldX * camera.zoom;
				if (nowScreenX >= -30 && nowScreenX <= w + 30) {
					ctx.fillStyle = accentColor;
					ctx.font = 'bold 11px -apple-system, sans-serif';
					ctx.fillText('Now', nowScreenX, stripH + 8);
				}
			}
			ctx.textAlign = 'start';
			ctx.textBaseline = 'alphabetic';
		}

		// -- Sticky thread labels (screen-space, fixed at left edge) --
		for (let i = 0; i < threads.length; i++) {
			const worldY = i * laneHeight + laneHeight / 2;
			const screenY = camera.panY + worldY * camera.zoom;
			if (screenY < -20 || screenY > h + 20) continue;

			const label = threads[i].name;
			ctx.font = '13px -apple-system, sans-serif';
			ctx.textBaseline = 'middle';
			const metrics = ctx.measureText(label);
			const padX = 6;
			const padY = 4;

			// Background pill for readability
			ctx.fillStyle = getCSS('--bg-panel') || '#1a1a24';
			ctx.globalAlpha = 0.85;
			ctx.beginPath();
			ctx.roundRect(16 - padX, screenY - 8 - padY, metrics.width + padX * 2, 16 + padY * 2, 4);
			ctx.fill();
			ctx.globalAlpha = 1.0;

			ctx.fillStyle = textPrimary;
			ctx.fillText(label, 16, screenY);
		}
	}

	// Pan handlers
	function handleCanvasPointerDown(e: PointerEvent) {
		if (e.button !== 0) return;
		const target = e.target as HTMLElement;
		// Don't start a pan if the user clicked a card or any interactive
		// element in the toolbar / new-thread popup. setPointerCapture would
		// otherwise steal the click event before it reached the button.
		if (target.closest('.canvas-card')) return;
		if (target.closest('.canvas-toolbar, .new-thread-popup, button, input, select, textarea, a')) return;
		panning = true;
		panStart = { x: e.clientX, y: e.clientY };
		panCameraStart = { x: canvas.camera.panX, y: canvas.camera.panY };
		containerEl.setPointerCapture(e.pointerId);
	}

	function handleCanvasPointerMove(e: PointerEvent) {
		if (!panning) return;
		const dx = e.clientX - panStart.x;
		const dy = e.clientY - panStart.y;
		panBy(
			panCameraStart.x + dx - canvas.camera.panX,
			panCameraStart.y + dy - canvas.camera.panY
		);
	}

	function handleCanvasPointerUp(e: PointerEvent) {
		if (panning) {
			const dx = Math.abs(e.clientX - panStart.x);
			const dy = Math.abs(e.clientY - panStart.y);
			panning = false;
			containerEl.releasePointerCapture(e.pointerId);

			// If the pointer barely moved, treat as a click — check message circle hit
			if (dx < 4 && dy < 4) {
				checkMessageClick(e.clientX, e.clientY);
			}
		}
	}

	/** Convert screen coords to world coords and check if a message circle was clicked. */
	function checkMessageClick(screenX: number, screenY: number) {
		const { panX, panY, zoom } = canvas.camera;
		const worldX = (screenX - panX) / zoom;
		const worldY = (screenY - panY) / zoom;
		// Match the capped on-screen radius used in drawBackground.
		const radiusFactor = zoom > MAX_VISUAL_ZOOM ? MAX_VISUAL_ZOOM / zoom : 1;
		const r = MSG_RADIUS * radiusFactor;

		for (const msg of canvas.messages) {
			const dx = worldX - msg.x;
			const dy = worldY - msg.y;
			if (dx * dx + dy * dy <= r * r) {
				// Open contact panel with this conversation
				app.contactPanelState = {
					contactId: msg.contact_id,
					conversationId: msg.conversation_id
				};
				return;
			}
		}
	}

	function handleWheel(e: WheelEvent) {
		e.preventDefault();
		// Conventions:
		//   Ctrl/Meta/Alt + wheel → zoom (Ctrl is the standard canvas pinch-to-zoom emulation)
		//   Shift + wheel        → horizontal pan
		//   plain wheel          → vertical pan
		if (e.ctrlKey || e.metaKey || e.altKey) {
			zoomAt(e.clientX, e.clientY, e.deltaY);
			return;
		}
		// Trackpads emit deltaX on horizontal scrolls; honor that even without modifier.
		const dx = e.shiftKey ? e.deltaY : e.deltaX;
		const dy = e.shiftKey ? 0 : e.deltaY;
		panBy(-dx, -dy);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'h' || e.key === 'H') {
			home();
		} else if (e.key === '+' || e.key === '=') {
			zoomAt(window.innerWidth / 2, window.innerHeight / 2, -100);
		} else if (e.key === '-') {
			zoomAt(window.innerWidth / 2, window.innerHeight / 2, 100);
		} else if (e.key === 'ArrowLeft') {
			panBy(50, 0);
		} else if (e.key === 'ArrowRight') {
			panBy(-50, 0);
		} else if (e.key === 'ArrowUp') {
			panBy(0, 50);
		} else if (e.key === 'ArrowDown') {
			panBy(0, -50);
		}
	}

	async function handleCreateThread() {
		const name = newThreadName.trim();
		if (!name) return;
		try {
			await apiCreateThread(name, '');
			newThreadName = '';
			showNewThread = false;
			canvasRefresh();
		} catch (e) {
			console.error('Failed to create thread:', e);
		}
	}

	async function handleDrop(e: DragEvent) {
		e.preventDefault();
		e.stopPropagation();
		dragOver = false;
		if (!e.dataTransfer?.files?.length) return;

		for (const file of e.dataTransfer.files) {
			try {
				const filePath = (file as any).path || file.name;
				await importFile(filePath);
			} catch (err) {
				console.error('Failed to import file:', err);
			}
		}
		canvasRefresh();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="canvas-container"
	bind:this={containerEl}
	onpointerdown={handleCanvasPointerDown}
	onpointermove={handleCanvasPointerMove}
	onpointerup={handleCanvasPointerUp}
	onwheel={handleWheel}
	ondragover={(e) => { e.preventDefault(); e.stopPropagation(); dragOver = true; }}
	ondragleave={(e) => { e.stopPropagation(); dragOver = false; }}
	ondrop={handleDrop}
>
	<!-- Background canvas layer -->
	<canvas class="bg-canvas" bind:this={canvasEl}></canvas>

	<!-- Card layer with CSS transform for pan/zoom -->
	<div
		class="card-layer"
		style="transform: translate({canvas.camera.panX}px, {canvas.camera.panY}px) scale({canvas.camera.zoom});"
	>
		{#each visibleDocs as doc (doc.id)}
			<CanvasCard
				{doc}
				isHovered={canvas.hoveredCardId === doc.id}
				isSelected={canvas.selectedCardId === doc.id}
				zoom={canvas.camera.zoom}
			/>
		{/each}
	</div>

	<!-- Canvas toolbar -->
	<div class="canvas-toolbar">
		<button class="toolbar-btn" onclick={() => (showNewThread = !showNewThread)} title="New thread">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<line x1="8" y1="3" x2="8" y2="13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
				<line x1="3" y1="8" x2="13" y2="8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
			</svg>
		</button>
	</div>

	{#if showNewThread}
		<div class="new-thread-popup">
			<input
				type="text"
				placeholder="Thread name..."
				bind:value={newThreadName}
				onkeydown={(e) => { if (e.key === 'Enter') handleCreateThread(); if (e.key === 'Escape') showNewThread = false; }}
				autofocus
			/>
			<button onclick={handleCreateThread}>Create</button>
		</div>
	{/if}

	<!-- Minimap overlay -->
	<Minimap />

	<!-- Loading / empty / error state -->
	{#if canvas.loadError}
		<div class="canvas-status" style="color: var(--error);">Error: {canvas.loadError}</div>
	{:else if !canvas.loaded}
		<div class="canvas-status">Loading canvas...</div>
	{:else if canvas.documents.length === 0}
		<div class="canvas-status">No documents yet. Create one via chat or search.</div>
	{/if}

	{#if dragOver}
		<div class="drop-overlay">
			<div class="drop-message">Drop files to import</div>
		</div>
	{/if}
</div>

<style>
	.canvas-container {
		position: relative;
		flex: 1;
		overflow: hidden;
		cursor: grab;
	}

	.canvas-container:active {
		cursor: grabbing;
	}

	.bg-canvas {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.card-layer {
		position: absolute;
		top: 0;
		left: 0;
		transform-origin: 0 0;
	}

	.canvas-toolbar {
		position: absolute;
		bottom: 8px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 4px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 4px 8px;
		z-index: 10;
	}

	.toolbar-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 4px;
		font-size: 0.85rem;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.toolbar-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.new-thread-popup {
		position: absolute;
		bottom: 52px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		gap: 8px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 8px 12px;
		z-index: 10;
	}

	.new-thread-popup input {
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-primary);
		padding: 4px 8px;
		font-size: 0.85rem;
		outline: none;
		width: 180px;
	}

	.new-thread-popup button {
		background: var(--accent);
		color: #000;
		border: none;
		border-radius: 4px;
		padding: 4px 12px;
		font-size: 0.85rem;
		cursor: pointer;
	}

	.canvas-status {
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		color: var(--text-muted);
		font-size: 0.9rem;
		pointer-events: none;
	}

	.drop-overlay {
		position: absolute;
		inset: 0;
		background: color-mix(in srgb, var(--info) 15%, transparent);
		border: 3px dashed var(--info);
		border-radius: 8px;
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 50;
		pointer-events: none;
	}

	.drop-message {
		font-size: 1.2rem;
		font-weight: 600;
		color: var(--info);
		background: var(--bg-panel);
		padding: 12px 24px;
		border-radius: 8px;
	}
</style>
