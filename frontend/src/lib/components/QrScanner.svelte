<script lang="ts">
	/** Camera-based QR scanner for the paired-device onboarding flow
	 *  (Phase 4.4). Uses `MediaDevices.getUserMedia` + `jsQR` — pure
	 *  JS, no native plugin needed beyond the Android camera permission
	 *  declared in `tauri.conf.json`.
	 *
	 *  Behaviour:
	 *    - On mount, request a back-facing camera at QVGA-ish resolution.
	 *    - Draw frames to an off-screen canvas at 5 fps and run jsQR.
	 *    - On a successful decode, fire `onDecode(payload)` exactly once
	 *      and stop the stream.
	 *    - On any failure (no camera, denied permission, no MediaDevices),
	 *      surface an error and let the parent fall back to its
	 *      textarea-paste UI.
	 *
	 *  The component is self-contained — parent passes a callback and
	 *  optionally a "Cancel" handler; styling is minimal so it can sit
	 *  inside the OnboardingWizard step layout. */
	import { onMount, onDestroy } from 'svelte';
	import jsQR from 'jsqr';

	let {
		onDecode,
		onCancel
	}: {
		onDecode: (payload: string) => void;
		onCancel?: () => void;
	} = $props();

	let videoEl = $state<HTMLVideoElement | null>(null);
	let stream: MediaStream | null = null;
	let scanInterval: ReturnType<typeof setInterval> | null = null;
	let canvas: HTMLCanvasElement | null = null;
	let ctx: CanvasRenderingContext2D | null = null;
	let error = $state('');
	let scanning = $state(false);

	async function startCamera() {
		if (!navigator.mediaDevices?.getUserMedia) {
			error = 'Camera API not available in this WebView.';
			return;
		}
		try {
			stream = await navigator.mediaDevices.getUserMedia({
				video: {
					facingMode: { ideal: 'environment' }, // back camera if available
					width: { ideal: 640 },
					height: { ideal: 480 }
				},
				audio: false
			});
			if (videoEl) {
				videoEl.srcObject = stream;
				await videoEl.play();
				scanning = true;
				canvas = document.createElement('canvas');
				ctx = canvas.getContext('2d', { willReadFrequently: true });
				// 5 fps is plenty for jsQR — the QR is held still while
				// the user lines it up.
				scanInterval = setInterval(scanFrame, 200);
			}
		} catch (e) {
			error = `Camera unavailable: ${e}`;
		}
	}

	function scanFrame() {
		if (!videoEl || !canvas || !ctx) return;
		if (videoEl.readyState !== videoEl.HAVE_ENOUGH_DATA) return;
		canvas.width = videoEl.videoWidth;
		canvas.height = videoEl.videoHeight;
		ctx.drawImage(videoEl, 0, 0, canvas.width, canvas.height);
		const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
		const code = jsQR(imageData.data, imageData.width, imageData.height, {
			inversionAttempts: 'dontInvert'
		});
		if (code) {
			scanning = false;
			stopCamera();
			onDecode(code.data);
		}
	}

	function stopCamera() {
		if (scanInterval) {
			clearInterval(scanInterval);
			scanInterval = null;
		}
		if (stream) {
			stream.getTracks().forEach((t) => t.stop());
			stream = null;
		}
		if (videoEl) {
			videoEl.srcObject = null;
		}
	}

	function handleCancel() {
		stopCamera();
		onCancel?.();
	}

	onMount(() => {
		startCamera();
	});

	onDestroy(() => {
		stopCamera();
	});
</script>

<div class="qr-scanner">
	{#if error}
		<div class="error-block">
			<p class="error">{error}</p>
			<p class="hint">Use the manual paste field below instead.</p>
		</div>
	{:else}
		<div class="video-frame">
			<!-- svelte-ignore a11y_media_has_caption -->
			<video bind:this={videoEl} autoplay playsinline muted></video>
			<div class="reticle"></div>
		</div>
		<p class="status">
			{scanning ? 'Point at the pairing QR shown on the existing device.' : 'Starting camera...'}
		</p>
		{#if onCancel}
			<button class="cancel-btn" onclick={handleCancel}>Use manual paste</button>
		{/if}
	{/if}
</div>

<style>
	.qr-scanner {
		display: flex;
		flex-direction: column;
		gap: 8px;
		align-items: center;
	}

	.video-frame {
		position: relative;
		width: 100%;
		max-width: 360px;
		aspect-ratio: 4 / 3;
		background: #000;
		border-radius: 8px;
		overflow: hidden;
	}

	video {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.reticle {
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: 60%;
		aspect-ratio: 1;
		border: 2px solid var(--accent, #f59e0b);
		border-radius: 12px;
		box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.3);
		pointer-events: none;
	}

	.status {
		font-size: 0.8rem;
		color: var(--text-muted, #888);
		margin: 0;
		text-align: center;
	}

	.cancel-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-secondary);
		padding: 6px 12px;
		font-size: 0.75rem;
		border-radius: 4px;
		cursor: pointer;
	}
	.cancel-btn:hover {
		color: var(--accent);
		border-color: var(--accent);
	}

	.error-block {
		padding: 12px;
		background: var(--bg-input);
		border-left: 3px solid var(--error, #ef4444);
		border-radius: 4px;
		width: 100%;
	}

	.error {
		color: var(--error, #ef4444);
		font-size: 0.85rem;
		margin: 0 0 4px 0;
	}

	.hint {
		font-size: 0.75rem;
		color: var(--text-muted);
		margin: 0;
	}
</style>
