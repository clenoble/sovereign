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

Security: CORS + the server-side origin check admit only the Tauri webview
origins (never wildcard — live camera frames must not be readable by arbitrary
websites). Set JIMINY_TOKEN (same value here and in the Sovereign app) to also
require a bearer token from non-browser clients.
"""
from __future__ import annotations

import argparse
import logging
import os
import secrets
import threading
import time
from dataclasses import dataclass
from typing import Optional

import cv2
import numpy as np
import uvicorn
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse, Response
from PIL import Image
from pydantic import BaseModel

logger = logging.getLogger("jiminy.vision")

DEFAULT_WINDOW_SECONDS = 300.0
DEFAULT_VLM_INTERVAL = 4.0
# A gesture must persist this many consecutive frames before it's committed to
# the shared state. At ~15-20 fps this is ~250-350ms — long enough to reject
# brief false positives (e.g. a hand passing the face read as "shush"), short
# enough that a real held sign still triggers barge-in fast.
GESTURE_STABLE_FRAMES = 5
# A "shush" only counts if the index fingertip is within this normalized distance
# of the mouth keypoint. Tight enough that a hand merely near the face doesn't
# qualify (it must actually be at the lips).
SHUSH_MAX_DIST = 0.10
# "Talking hand" (chatterbox) trigger — a hand miming a speaking mouth, the thumb
# opening/closing against the four fingers. Detected dynamically from the
# normalized thumb-to-fingertips gap ("openness"). Because that gap's absolute
# range depends heavily on hand orientation (open/close along the camera's depth
# axis barely moves in 2D), we detect *relative* oscillation: count direction
# reversals whose size exceeds TALK_MIN_AMPLITUDE; fire on >= TALK_MIN_TOGGLES
# within TALK_WINDOW_S, then stay "active" for TALK_HOLD_S so the frame-debounce
# can commit it. This adapts to whatever swing magnitude the hand produces while
# still rejecting small mid-range jitter.
TALK_MIN_AMPLITUDE = 0.22
TALK_MIN_TOGGLES = 3
TALK_WINDOW_S = 2.5
TALK_HOLD_S = 0.8
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
        self._talking = TalkingHandDetector()
        self.last_openness: Optional[float] = None  # debug/tuning aid

    def detect(self, frame_bgr: np.ndarray, now: float) -> Optional[str]:
        rgb = cv2.cvtColor(frame_bgr, cv2.COLOR_BGR2RGB)
        img = self._mp.Image(image_format=self._mp.ImageFormat.SRGB, data=rgb)
        grec = self._rec.recognize(img)
        if not grec.hand_landmarks:
            self.last_openness = None
            return None
        lm = grec.hand_landmarks[0]

        # Dynamic 'talking hand' trigger takes priority while actively miming.
        openness = self._hand_openness(lm)
        self.last_openness = openness
        if self._talking.update(openness, now):
            return "talking_hand"

        builtin = (grec.gestures[0][0].category_name
                   if grec.gestures and grec.gestures[0] else None)

        mouth = None
        fdet = self._face.detect(img)
        if fdet.detections and len(fdet.detections[0].keypoints) > 3:
            kp = fdet.detections[0].keypoints[3]  # BlazeFace mouth keypoint
            mouth = (kp.x, kp.y)

        return self.classify_gesture(
            (lm[8].x, lm[8].y), (lm[6].x, lm[6].y), builtin, mouth,
            others_folded=self._others_folded(lm))

    @staticmethod
    def _hand_openness(lm) -> float:
        """Normalized gap between the thumb tip and the four-fingertip centroid
        (the 'mouth' opening of a talking hand), scaled by hand size (wrist →
        middle-finger MCP) so it's invariant to distance from the camera."""
        fx = (lm[8].x + lm[12].x + lm[16].x + lm[20].x) / 4.0
        fy = (lm[8].y + lm[12].y + lm[16].y + lm[20].y) / 4.0
        gap = ((lm[4].x - fx) ** 2 + (lm[4].y - fy) ** 2) ** 0.5
        scale = ((lm[0].x - lm[9].x) ** 2 + (lm[0].y - lm[9].y) ** 2) ** 0.5
        return gap / scale if scale > 1e-6 else 0.0

    @staticmethod
    def _others_folded(lm) -> bool:
        """True when the middle, ring and pinky fingers are curled (tip below its
        PIP joint in image space). A real 'quiet' sign is index-only; requiring
        the other fingers folded rejects an open hand / wave passing the face."""
        return (lm[12].y > lm[10].y    # middle folded
                and lm[16].y > lm[14].y  # ring folded
                and lm[20].y > lm[18].y)  # pinky folded

    @staticmethod
    def classify_gesture(index_tip, index_pip, builtin, mouth, others_folded=True):
        """Map hand landmarks + the built-in category to a gesture name.

        `shush` takes priority over the built-in category but requires ALL of:
        an extended index finger (tip above its PIP), the other fingers folded,
        and the fingertip within `SHUSH_MAX_DIST` of the mouth keypoint — so a
        hand merely near the face isn't misread as a quiet sign. Otherwise the
        built-in is mapped via _MAP. Coordinates are normalized (x, y) in [0, 1];
        `mouth` may be None.
        """
        if mouth is not None and others_folded and index_tip[1] < index_pip[1]:
            d = ((index_tip[0] - mouth[0]) ** 2 + (index_tip[1] - mouth[1]) ** 2) ** 0.5
            if d < SHUSH_MAX_DIST:
                return "shush"
        return GestureDetector._MAP.get(builtin)


class GestureDebouncer:
    """Suppress flickery / single-frame gesture readings.

    A raw per-frame detection is only *committed* once the same value has been
    seen for `stable_frames` consecutive reads. Crucially this also applies to
    ``None``: a sustained run of "no gesture" commits ``None``, clearing a
    previously-held sign so it can't latch forever (the old bug where one stray
    "shush" frame stuck in the shared state and silently stopped speech).
    """

    def __init__(self, stable_frames: int = GESTURE_STABLE_FRAMES) -> None:
        self.stable_frames = max(1, stable_frames)
        self._raw = None        # last raw value seen
        self._streak = 0        # consecutive reads of self._raw
        self.committed = None   # last value that reached the stability threshold

    def update(self, raw: Optional[str]) -> Optional[str]:
        """Feed one raw detection; return the currently committed gesture."""
        if raw == self._raw:
            self._streak += 1
        else:
            self._raw = raw
            self._streak = 1
        if self._streak >= self.stable_frames:
            self.committed = raw
        return self.committed


class TalkingHandDetector:
    """Detects the 'talking hand' / chatterbox gesture — a hand miming a mouth,
    the thumb repeatedly opening and closing against the four fingers.

    Fed the normalized thumb-to-fingertips gap (the 'mouth' opening) each frame,
    it tracks *relative* oscillation: it follows the running extreme and registers
    a direction reversal whenever the signal turns back by at least
    `min_amplitude`. On `min_toggles` reversals within `window_s` it fires, then
    reports active for `hold_s` so the frame-debounce can commit a "talking_hand"
    gesture. Using relative amplitude (not absolute thresholds) makes it robust to
    the gap's orientation-dependent scale, while still rejecting small jitter and
    a static hand (which never reverses).
    """

    def __init__(self, min_amplitude: float = TALK_MIN_AMPLITUDE,
                 min_toggles: int = TALK_MIN_TOGGLES,
                 window_s: float = TALK_WINDOW_S, hold_s: float = TALK_HOLD_S) -> None:
        self.min_amplitude = min_amplitude
        self.min_toggles = max(1, min_toggles)
        self.window_s = window_s
        self.hold_s = hold_s
        self._pivot: Optional[float] = None  # running extreme in the current direction
        self._rising: Optional[bool] = None  # True=rising, False=falling, None=undecided
        self._toggles: list[float] = []      # timestamps of recent reversals
        self._active_until = 0.0

    def update(self, openness: float, now: float) -> bool:
        """Feed one frame's openness; return whether 'talking' is currently active."""
        if self._pivot is None:
            self._pivot = openness
        elif self._rising is None:
            # Establish a direction once the signal moves >= min_amplitude from
            # the starting pivot (no reversal counted for the first move).
            if openness - self._pivot >= self.min_amplitude:
                self._rising, self._pivot = True, openness
            elif self._pivot - openness >= self.min_amplitude:
                self._rising, self._pivot = False, openness
        elif self._rising:
            if openness > self._pivot:
                self._pivot = openness                      # extend the rise
            elif self._pivot - openness >= self.min_amplitude:
                self._toggles.append(now)                   # peak -> reverse down
                self._rising, self._pivot = False, openness
        else:  # falling
            if openness < self._pivot:
                self._pivot = openness                      # extend the fall
            elif openness - self._pivot >= self.min_amplitude:
                self._toggles.append(now)                   # trough -> reverse up
                self._rising, self._pivot = True, openness

        # Keep only reversals within the sliding window.
        self._toggles = [t for t in self._toggles if now - t <= self.window_s]
        if len(self._toggles) >= self.min_toggles:
            self._active_until = now + self.hold_s
            self._toggles.clear()  # require fresh oscillation before firing again
        return now < self._active_until


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
    hand_openness: Optional[float] = None  # latest talking-hand openness (tuning aid)


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
        self._debounce = GestureDebouncer()
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
                "hand_openness": round(s.hand_openness, 3) if s.hand_openness is not None else None,
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
                gesture = self.detector.detect(frame, t)
            except Exception as e:  # never let detection kill the loop
                logger.debug("gesture detect error: %s", e)
            # Debounce: only commit a gesture (or its absence) after it's stable
            # for a few frames, so one stray frame can't latch or false-trigger.
            committed = self._debounce.update(gesture)
            with self._lock:
                self._frame = frame
                self.state.camera_ok = True
                self.state.last_frame_ts = t
                self.state.gesture = committed
                self.state.hand_openness = getattr(self.detector, "last_openness", None)
                if committed is not None:
                    self.state.gesture_ts = t  # refresh age while the sign is held
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

# The Tauri webview fetches /vision/* cross-origin (state + window control), so
# CORS must admit it — but ONLY it. A wildcard here would let any website the
# user visits read live camera frames from the loopback service.
ALLOWED_ORIGINS = [
    "tauri://localhost",        # macOS / Linux webview origin
    "http://tauri.localhost",   # Windows (WebView2) origin
    "https://tauri.localhost",
]
app.add_middleware(
    CORSMiddleware,
    allow_origins=ALLOWED_ORIGINS,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Shared secret. Resolved from JIMINY_TOKEN, else the shared token file the
# Sovereign app writes (JIMINY_TOKEN_FILE override, default
# ~/.sovereign/crypto/jiminy_token) so the secure default needs no manual env
# coordination. The webview sends it as a bearer token on its /vision fetches.
def _resolve_auth_token() -> str:
    tok = os.environ.get("JIMINY_TOKEN", "")
    if tok:
        return tok
    path = os.environ.get("JIMINY_TOKEN_FILE") or os.path.join(
        os.path.expanduser("~"), ".sovereign", "crypto", "jiminy_token"
    )
    try:
        with open(path, "r", encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return ""


AUTH_TOKEN = _resolve_auth_token()
# SIDECAR-002: fail closed when no token is configured (don't serve live camera
# frames / scene state openly). JIMINY_ALLOW_NO_AUTH=1 opts into insecure dev.
ALLOW_NO_AUTH = os.environ.get("JIMINY_ALLOW_NO_AUTH", "") == "1"


def _auth_error(origin: Optional[str], authorization: Optional[str]) -> Optional[str]:
    """Return an error string if the request must be rejected, else None.

    CORS only stops a page READING responses; simple GETs/POSTs still reach the
    handlers, so the Origin allowlist is enforced server-side as well.
    """
    # SIDECAR-001: the Origin allowlist and the token are AND-composed, not
    # mutually exclusive. A page cannot forge its Origin, but a NON-browser
    # local process can trivially set `Origin: tauri://localhost`, so the Origin
    # check alone is necessary-not-sufficient. The token must ALWAYS be checked
    # when configured, regardless of Origin — previously it sat in an `elif`, so
    # any allow-listed Origin skipped it and any local process read the camera.
    if origin is not None and origin not in ALLOWED_ORIGINS:
        return "origin not allowed"
    if not AUTH_TOKEN:
        # SIDECAR-002: no shared secret → fail closed unless explicitly opted out.
        return None if ALLOW_NO_AUTH else "sidecar auth not configured (set JIMINY_TOKEN); refusing"
    scheme, _, token = (authorization or "").partition(" ")
    if scheme != "Bearer" or not secrets.compare_digest(token, AUTH_TOKEN):
        return "missing or invalid bearer token"
    return None


@app.middleware("http")
async def require_local_auth(request: Request, call_next):
    err = _auth_error(request.headers.get("origin"), request.headers.get("authorization"))
    if err is not None:
        return JSONResponse(status_code=403, content={"detail": err})
    return await call_next(request)


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
