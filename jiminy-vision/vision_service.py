"""Jiminy Vision service — turns what Jiminy sees into orchestrator context.

Runs in its own venv (heavy deps: torch / transformers / mediapipe) so it can't
disturb the reachy sidecar. Two layers:

  * Always-on GESTURE detection (MediaPipe) — fast, every frame. Surfaces
    discrete signs (shush, open_palm/stop, point, fist) the orchestrator reacts
    to instantly (e.g. shush -> stop speaking).
  * Windowed SCENE understanding (SmolVLM2) — heavy. Off by default; a UI
    control or a trigger gesture opens a time window (default 300s) during which
    frames are periodically captioned and the description is exposed as context.

Frame source: the local PC webcam by default (dev / desktop mode), or the robot
camera by polling the reachy sidecar's /camera/frame on hardware.

The Rust orchestrator POLLS GET /vision/state (like it polls the camera) and
converts it into OrchestratorEvents. POST /vision/window opens the VLM window.
"""
from __future__ import annotations

import argparse
import logging
import os
import threading
import time
from dataclasses import dataclass
from typing import Optional

import cv2
import numpy as np
import uvicorn
from fastapi import FastAPI
from fastapi.responses import Response
from PIL import Image
from pydantic import BaseModel

logger = logging.getLogger("jiminy.vision")

DEFAULT_WINDOW_SECONDS = 300.0
DEFAULT_VLM_INTERVAL = 4.0
VLM_PROMPT = (
    "You are the eyes of an assistant. In one short sentence, describe the person "
    "in view and any gesture, hand sign, or facial expression they are making "
    "(for example: waving, pointing, a shush/quiet sign, thumbs up, nodding)."
)


# --------------------------------------------------------------------------- #
# Frame sources
# --------------------------------------------------------------------------- #
class WebcamSource:
    """Local PC webcam via OpenCV (dev / desktop mode)."""

    def __init__(self, index: int = 0) -> None:
        # CAP_DSHOW avoids slow MSMF startup on Windows.
        self.cap = cv2.VideoCapture(index, cv2.CAP_DSHOW)
        self.name = f"webcam:{index}"

    def read(self) -> Optional[np.ndarray]:
        if not self.cap.isOpened():
            return None
        ok, frame = self.cap.read()
        return frame if ok else None

    def release(self) -> None:
        self.cap.release()


class RobotSource:
    """Robot camera via the reachy sidecar's /camera/frame JPEG endpoint."""

    def __init__(self, base_url: str) -> None:
        import requests  # local import; only needed for this source
        self._requests = requests
        self.base_url = base_url.rstrip("/")
        self.name = f"robot:{self.base_url}"

    def read(self) -> Optional[np.ndarray]:
        try:
            r = self._requests.get(self.base_url + "/camera/frame", timeout=2)
            if r.status_code != 200:
                return None
            buf = np.frombuffer(r.content, dtype=np.uint8)
            return cv2.imdecode(buf, cv2.IMREAD_COLOR)
        except Exception:
            return None

    def release(self) -> None:
        pass


# --------------------------------------------------------------------------- #
# Gesture detection (MediaPipe, always on)
# --------------------------------------------------------------------------- #
def _ensure_model(model_dir: str, name: str, url: str) -> str:
    """Download a MediaPipe .task/.tflite model into model_dir if missing."""
    import urllib.request
    os.makedirs(model_dir, exist_ok=True)
    path = os.path.join(model_dir, name)
    if not os.path.isfile(path):
        logger.info("Downloading MediaPipe model %s ...", name)
        urllib.request.urlretrieve(url, path)
    return path


class GestureDetector:
    """Classifies a single dominant hand sign from a BGR frame (MediaPipe Tasks).

    Built-in gestures (Open_Palm / Closed_Fist / Pointing_Up / Thumb_Up / Victory
    / ...) come from the GestureRecognizer; "shush" is custom — an extended index
    finger whose tip sits at the mouth (located via the FaceDetector keypoint).
    """

    GESTURE_URL = ("https://storage.googleapis.com/mediapipe-models/gesture_recognizer/"
                   "gesture_recognizer/float16/latest/gesture_recognizer.task")
    FACE_URL = ("https://storage.googleapis.com/mediapipe-models/face_detector/"
                "blaze_face_short_range/float16/latest/blaze_face_short_range.tflite")
    _MAP = {"Open_Palm": "open_palm", "Closed_Fist": "fist", "Pointing_Up": "point",
            "Thumb_Up": "thumbs_up", "Thumb_Down": "thumbs_down", "Victory": "victory",
            "ILoveYou": "iloveyou"}

    def __init__(self, model_dir: Optional[str] = None) -> None:
        import mediapipe as mp
        from mediapipe.tasks import python as mp_python
        from mediapipe.tasks.python import vision as mp_vision
        self._mp = mp
        model_dir = model_dir or os.path.join(
            os.path.dirname(os.path.abspath(__file__)), "models")
        gpath = _ensure_model(model_dir, "gesture_recognizer.task", self.GESTURE_URL)
        fpath = _ensure_model(model_dir, "blaze_face_short_range.tflite", self.FACE_URL)
        self._rec = mp_vision.GestureRecognizer.create_from_options(
            mp_vision.GestureRecognizerOptions(
                base_options=mp_python.BaseOptions(model_asset_path=gpath), num_hands=1))
        self._face = mp_vision.FaceDetector.create_from_options(
            mp_vision.FaceDetectorOptions(
                base_options=mp_python.BaseOptions(model_asset_path=fpath)))

    def detect(self, frame_bgr: np.ndarray) -> Optional[str]:
        rgb = cv2.cvtColor(frame_bgr, cv2.COLOR_BGR2RGB)
        img = self._mp.Image(image_format=self._mp.ImageFormat.SRGB, data=rgb)
        grec = self._rec.recognize(img)
        if not grec.hand_landmarks:
            return None
        lm = grec.hand_landmarks[0]
        builtin = (grec.gestures[0][0].category_name
                   if grec.gestures and grec.gestures[0] else None)

        mouth = None
        fdet = self._face.detect(img)
        if fdet.detections and len(fdet.detections[0].keypoints) > 3:
            kp = fdet.detections[0].keypoints[3]  # BlazeFace mouth keypoint
            mouth = (kp.x, kp.y)

        return self.classify_gesture(
            (lm[8].x, lm[8].y), (lm[6].x, lm[6].y), builtin, mouth)

    @staticmethod
    def classify_gesture(index_tip, index_pip, builtin, mouth):
        """Map hand landmarks + the built-in category to a gesture name.

        `shush` (an extended index finger whose tip sits at the mouth) takes
        priority over the built-in category; otherwise the built-in is mapped via
        _MAP. Coordinates are normalized (x, y) in [0, 1]; `mouth` may be None.
        """
        if mouth is not None and index_tip[1] < index_pip[1]:
            d = ((index_tip[0] - mouth[0]) ** 2 + (index_tip[1] - mouth[1]) ** 2) ** 0.5
            if d < 0.12:
                return "shush"
        return GestureDetector._MAP.get(builtin)


# --------------------------------------------------------------------------- #
# Scene understanding (SmolVLM2, windowed)
# --------------------------------------------------------------------------- #
class VlmCaptioner:
    """Lazy-loaded SmolVLM2 captioner. Loaded on first use to save VRAM."""

    def __init__(self, model_id: str, device: str) -> None:
        self.model_id = model_id
        self.device = device
        self._proc = None
        self._model = None
        self._dtype = None
        self._load_lock = threading.Lock()

    def ensure_loaded(self) -> None:
        if self._model is not None:
            return
        with self._load_lock:
            if self._model is not None:
                return
            import torch
            from transformers import AutoModelForImageTextToText, AutoProcessor
            self._dtype = torch.float16 if self.device == "cuda" else torch.float32
            logger.info("Loading SmolVLM2 %s on %s (%s)...",
                        self.model_id, self.device, self._dtype)
            self._proc = AutoProcessor.from_pretrained(self.model_id)
            self._model = AutoModelForImageTextToText.from_pretrained(
                self.model_id, dtype=self._dtype, _attn_implementation="eager",
            ).to(self.device)
            self._model.eval()
            logger.info("SmolVLM2 loaded.")

    def caption(self, frame_bgr: np.ndarray, prompt: str = VLM_PROMPT,
                max_new_tokens: int = 72) -> str:
        self.ensure_loaded()
        import torch
        rgb = cv2.cvtColor(frame_bgr, cv2.COLOR_BGR2RGB)
        pil = Image.fromarray(rgb)
        pil = _resize_max(pil, 768)
        messages = [{"role": "user", "content": [
            {"type": "image"}, {"type": "text", "text": prompt}]}]
        text = self._proc.apply_chat_template(messages, add_generation_prompt=True)
        inputs = self._proc(text=text, images=[pil], return_tensors="pt").to(self.device)
        with torch.no_grad():
            ids = self._model.generate(**inputs, do_sample=False,
                                       max_new_tokens=max_new_tokens)
        out = self._proc.batch_decode(ids, skip_special_tokens=True)[0]
        return _extract_answer(out)


def _resize_max(img: Image.Image, longest: int) -> Image.Image:
    w, h = img.size
    if max(w, h) <= longest:
        return img
    s = longest / max(w, h)
    return img.resize((int(w * s), int(h * s)), Image.LANCZOS)


def _extract_answer(text: str) -> str:
    # SmolVLM decodes the whole conversation; keep only the assistant turn.
    for marker in ("Assistant:", "assistant\n", "assistant:"):
        if marker in text:
            text = text.split(marker)[-1]
    return text.strip()


# --------------------------------------------------------------------------- #
# Shared state + worker loops
# --------------------------------------------------------------------------- #
@dataclass
class VisionState:
    gesture: Optional[str] = None
    gesture_ts: float = 0.0
    scene: str = ""
    scene_ts: float = 0.0
    window_until: float = 0.0
    camera_ok: bool = False
    last_frame_ts: float = 0.0


class VisionEngine:
    def __init__(self, source, detector: GestureDetector, vlm: VlmCaptioner,
                 window_seconds: float, vlm_interval: float, clock=time.time) -> None:
        self.source = source
        self.detector = detector
        self.vlm = vlm
        self.window_seconds = window_seconds
        self.vlm_interval = vlm_interval
        self.now = clock
        self.state = VisionState()
        self._frame: Optional[np.ndarray] = None
        self._lock = threading.Lock()
        self._stop = threading.Event()

    # -- window control --
    def open_window(self, duration: Optional[float] = None) -> float:
        until = self.now() + (duration or self.window_seconds)
        with self._lock:
            self.state.window_until = until
        return until

    def close_window(self) -> None:
        with self._lock:
            self.state.window_until = 0.0

    def window_active(self) -> bool:
        return self.now() < self.state.window_until

    # -- snapshot for the API --
    def snapshot(self) -> dict:
        with self._lock:
            s = self.state
            remaining = max(0.0, s.window_until - self.now())
            return {
                "gesture": s.gesture,
                "gesture_age_s": round(self.now() - s.gesture_ts, 2) if s.gesture_ts else None,
                "scene": s.scene,
                "scene_age_s": round(self.now() - s.scene_ts, 2) if s.scene_ts else None,
                "window_active": remaining > 0,
                "window_remaining_s": round(remaining, 1),
                "camera_ok": s.camera_ok,
                "frame_age_s": round(self.now() - s.last_frame_ts, 2) if s.last_frame_ts else None,
            }

    def latest_jpeg(self, quality: int = 70) -> Optional[bytes]:
        with self._lock:
            frame = None if self._frame is None else self._frame.copy()
        if frame is None:
            return None
        ok, jpg = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, quality])
        return jpg.tobytes() if ok else None

    # -- threads --
    def start(self) -> None:
        threading.Thread(target=self._capture_loop, name="vision-capture", daemon=True).start()
        threading.Thread(target=self._vlm_loop, name="vision-vlm", daemon=True).start()

    def stop(self) -> None:
        self._stop.set()

    def _capture_loop(self) -> None:
        while not self._stop.is_set():
            frame = self.source.read()
            t = self.now()
            if frame is None:
                with self._lock:
                    self.state.camera_ok = False
                time.sleep(0.2)
                continue
            gesture = None
            try:
                gesture = self.detector.detect(frame)
            except Exception as e:  # never let detection kill the loop
                logger.debug("gesture detect error: %s", e)
            with self._lock:
                self._frame = frame
                self.state.camera_ok = True
                self.state.last_frame_ts = t
                if gesture is not None:
                    self.state.gesture = gesture
                    self.state.gesture_ts = t
            time.sleep(0.05)  # ~20 fps cap

    def _vlm_loop(self) -> None:
        last = 0.0
        while not self._stop.is_set():
            if self.window_active() and (self.now() - last) >= self.vlm_interval:
                with self._lock:
                    frame = None if self._frame is None else self._frame.copy()
                if frame is not None:
                    try:
                        desc = self.vlm.caption(frame)
                        with self._lock:
                            self.state.scene = desc
                            self.state.scene_ts = self.now()
                        logger.info("scene: %s", desc)
                    except Exception as e:
                        logger.warning("VLM caption failed: %s", e)
                last = self.now()
            time.sleep(0.5)


# --------------------------------------------------------------------------- #
# FastAPI app
# --------------------------------------------------------------------------- #
engine: Optional[VisionEngine] = None
app = FastAPI(title="Jiminy Vision", version="0.1.0")


class WindowRequest(BaseModel):
    duration_s: Optional[float] = None


@app.get("/health")
def health():
    return {"ok": True, "camera": engine.source.name if engine else None}


@app.get("/vision/state")
def vision_state():
    return engine.snapshot()


@app.post("/vision/window")
def vision_window(req: WindowRequest):
    until = engine.open_window(req.duration_s)
    return {"window_active": True, "window_remaining_s": round(until - time.time(), 1)}


@app.post("/vision/window/stop")
def vision_window_stop():
    engine.close_window()
    return {"window_active": False}


@app.get("/vision/frame")
def vision_frame(quality: int = 70):
    jpg = engine.latest_jpeg(quality)
    if jpg is None:
        return Response(status_code=503)
    return Response(content=jpg, media_type="image/jpeg")


# --------------------------------------------------------------------------- #
def main() -> None:
    global engine
    ap = argparse.ArgumentParser(description="Jiminy Vision service")
    ap.add_argument("--port", type=int, default=9101)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--camera", choices=["webcam", "robot"], default="webcam",
                    help="Frame source: local PC webcam (dev) or robot camera via sidecar")
    ap.add_argument("--webcam-index", type=int, default=0)
    ap.add_argument("--sidecar-url", default="http://127.0.0.1:9100")
    ap.add_argument("--window-seconds", type=float, default=DEFAULT_WINDOW_SECONDS)
    ap.add_argument("--vlm-interval", type=float, default=DEFAULT_VLM_INTERVAL)
    ap.add_argument("--vlm-model", default="HuggingFaceTB/SmolVLM2-2.2B-Instruct")
    ap.add_argument("--device", default=None, help="cuda | cpu (default: auto)")
    args = ap.parse_args()

    logging.basicConfig(level=logging.INFO,
                        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s")

    if args.device:
        device = args.device
    else:
        try:
            import torch
            device = "cuda" if torch.cuda.is_available() else "cpu"
        except Exception:
            device = "cpu"

    source = (WebcamSource(args.webcam_index) if args.camera == "webcam"
              else RobotSource(args.sidecar_url))
    detector = GestureDetector()
    vlm = VlmCaptioner(args.vlm_model, device)
    engine = VisionEngine(source, detector, vlm, args.window_seconds, args.vlm_interval)
    engine.start()

    logger.info("Jiminy Vision on %s:%d (camera=%s, device=%s, window=%.0fs)",
                args.host, args.port, source.name, device, args.window_seconds)
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
