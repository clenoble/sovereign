# Jiminy Vision

Turns what Jiminy sees into context for the AI orchestrator. Runs as a **separate
service with its own venv** so its heavy ML deps (torch / torchvision /
transformers / mediapipe) can't disturb the reachy sidecar (`jiminy-bridge`).

Two layers:

- **Always-on gesture detection** (MediaPipe Tasks) — fast, every frame. Surfaces
  discrete signs the orchestrator reacts to instantly: `shush` (index finger at
  the mouth → stop speaking), `open_palm` (stop), `point`, `fist`, `thumbs_up`,
  `thumbs_down`, `victory`, `iloveyou`.
- **Windowed scene understanding** (SmolVLM2, ~2.2B) — heavy, off by default. A UI
  control or a trigger gesture opens a time **window** (default 300s, configurable)
  during which frames are periodically captioned and the description is exposed as
  orchestrator context. The VLM is lazy-loaded on first use to save VRAM.

**Frame source:** the local PC webcam by default (dev / desktop mode), or the
robot camera by polling the reachy sidecar's `/camera/frame` on hardware.

## Setup

```bash
python -m venv .venv
.venv/Scripts/python -m pip install torch torchvision --index-url https://download.pytorch.org/whl/cu124
.venv/Scripts/python -m pip install -r requirements.txt
```

MediaPipe `.task` models and the SmolVLM2 weights download automatically on first
run (into `models/` and the HuggingFace cache respectively).

## Run

```bash
.venv/Scripts/python vision_service.py --camera webcam     # dev: PC webcam
.venv/Scripts/python vision_service.py --camera robot      # robot cam via sidecar
.venv/Scripts/python vision_service.py --window-seconds 300 --vlm-interval 4
```

## API (polled / driven by the Rust orchestrator)

| Method | Path | Purpose |
|--------|------|---------|
| GET  | `/vision/state`        | latest gesture + scene caption + window status |
| POST | `/vision/window`       | `{duration_s?}` open the VLM window (default 300s) |
| POST | `/vision/window/stop`  | close the window |
| GET  | `/vision/frame`        | latest JPEG (camera tile) |
| GET  | `/health`              | liveness + camera name |

## Notes

- **transformers must be 4.x** — 5.x dropped `SmolVLMImageProcessor`.
- **torchvision is required** by SmolVLM2's image/video processor.
- GPU: float16 + eager attention (the GTX 1660 / Turing has no bf16 or
  flash-attention-2). Falls back to CPU float32 when CUDA is unavailable.
