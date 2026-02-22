"""Jiminy Bridge — FastAPI sidecar connecting Sovereign OS to Reachy Mini.

Sovereign OS (Rust) sends HTTP commands here, and this server translates
them into Reachy Mini SDK calls. Supports both simulation (MuJoCo) and
real hardware.

Usage:
    pip install -r requirements.txt
    python main.py                    # Auto-detect (sim or hardware)
    python main.py --sim              # Force simulation mode
    python main.py --port 9100        # Custom port

Simulation:
    Start daemon with gold-themed scene:
        reachy-mini-daemon --sim --scene sovereign
    (Install scene first: copy scenes/sovereign.xml into
     .venv/Lib/site-packages/reachy_mini/descriptions/reachy_mini/mjcf/scenes/)
"""

from __future__ import annotations

import argparse
import asyncio
import logging
from contextlib import asynccontextmanager
from typing import Optional

import numpy as np
import uvicorn
from fastapi import FastAPI, HTTPException, WebSocket
from fastapi.responses import Response
from pydantic import BaseModel

from reachy_mini import ReachyMini
from reachy_mini.utils import create_head_pose

from audio_stream import audio_ws_handler
from emotions import EmotionLibrary
from tts_engine import PiperTts, create_tts_engine

logger = logging.getLogger("jiminy")

# --- Request models ---


class EmotionRequest(BaseModel):
    name: str


class DanceRequest(BaseModel):
    name: str


class SpeakRequest(BaseModel):
    text: str


class PoseRequest(BaseModel):
    head_roll: float = 0.0
    head_pitch: float = 0.0
    head_yaw: float = 0.0
    antenna_left: float = 0.0
    antenna_right: float = 0.0
    body_yaw: float = 0.0
    duration: float = 0.5


class LookAtRequest(BaseModel):
    x: float
    y: float
    z: float


# --- Global state ---

mini: Optional[ReachyMini] = None
library: Optional[EmotionLibrary] = None
tts_engine: Optional[PiperTts] = None
_idle_task: Optional[asyncio.Task] = None


# --- Lifespan ---


@asynccontextmanager
async def lifespan(app: FastAPI):
    global mini, library
    logger.info("Starting Jiminy bridge...")

    # Connect to robot (daemon must be running separately)
    # Start daemon first: reachy-mini-daemon        (USB hardware)
    #                   or reachy-mini-daemon --sim  (MuJoCo simulation)
    try:
        mini = ReachyMini(media_backend="default")
        mini.__enter__()
        logger.info("Connected to Reachy Mini")
    except Exception as e:
        logger.error("Failed to connect to Reachy Mini: %s", e)
        logger.info("Continuing without robot — status endpoint will report disconnected")
        mini = None

    # Load emotion/dance libraries
    library = EmotionLibrary()
    try:
        library.load()
    except Exception as e:
        logger.warning("Failed to load emotion libraries: %s", e)

    # Initialize TTS engine (optional — needs PIPER_MODEL env)
    global tts_engine
    tts_engine = create_tts_engine()

    yield

    # Shutdown
    if mini is not None:
        try:
            mini.__exit__(None, None, None)
        except Exception:
            pass
    logger.info("Jiminy bridge stopped")


app = FastAPI(title="Jiminy Bridge", version="0.1.0", lifespan=lifespan)


# --- Endpoints ---


@app.get("/status")
async def status():
    """Check robot connection health."""
    emotions = library.list_emotions() if library else []
    dances = library.list_dances() if library else []
    return {
        "connected": mini is not None,
        "emotions_loaded": len(emotions),
        "dances_loaded": len(dances),
        "available_emotions": emotions[:10],
        "available_dances": dances[:10],
    }


@app.post("/emotion")
async def play_emotion(req: EmotionRequest):
    """Play a named emotion animation."""
    if mini is None:
        raise HTTPException(503, "Robot not connected")
    if library is None:
        raise HTTPException(503, "Emotion library not loaded")
    loop = asyncio.get_event_loop()
    played = await loop.run_in_executor(None, library.play_emotion, mini, req.name)
    if not played:
        available = library.list_emotions()
        raise HTTPException(
            404,
            f"Emotion '{req.name}' not found. Available: {available[:10]}",
        )
    return {"status": "ok", "emotion": req.name}


@app.post("/dance")
async def play_dance(req: DanceRequest):
    """Play a named dance animation."""
    if mini is None:
        raise HTTPException(503, "Robot not connected")
    if library is None:
        raise HTTPException(503, "Dance library not loaded")
    loop = asyncio.get_event_loop()
    played = await loop.run_in_executor(None, library.play_dance, mini, req.name)
    if not played:
        available = library.list_dances()
        raise HTTPException(
            404,
            f"Dance '{req.name}' not found. Available: {available[:10]}",
        )
    return {"status": "ok", "dance": req.name}


@app.post("/pose")
async def set_pose(req: PoseRequest):
    """Move to a specific pose with interpolation."""
    if mini is None:
        raise HTTPException(503, "Robot not connected")
    head = create_head_pose(
        roll=np.deg2rad(req.head_roll),
        pitch=np.deg2rad(req.head_pitch),
        yaw=np.deg2rad(req.head_yaw),
        degrees=False,
        mm=False,
    )
    mini.goto_target(
        head=head,
        antennas=[req.antenna_left, req.antenna_right],
        body_yaw=np.deg2rad(req.body_yaw),
        duration=req.duration,
    )
    return {"status": "ok"}


@app.post("/idle")
async def idle():
    """Return to neutral idle position."""
    if mini is None:
        raise HTTPException(503, "Robot not connected")
    head = create_head_pose(roll=0.0, pitch=0.0, yaw=0.0, degrees=False, mm=False)
    mini.goto_target(head=head, antennas=[0.0, 0.0], body_yaw=0.0, duration=1.0)
    return {"status": "ok"}


@app.post("/speak")
async def speak(req: SpeakRequest):
    """Speak text through the robot's speaker via Piper TTS.

    Falls back to antenna animation if TTS is not configured.
    """
    if mini is None:
        raise HTTPException(503, "Robot not connected")
    logger.info("Speaking: %s", req.text[:80])

    if tts_engine is not None:
        # Real TTS: generate speech + animate antennas concurrently
        async def animate_antennas():
            try:
                while True:
                    mini.set_target(antennas=[0.3, 0.3])
                    await asyncio.sleep(0.2)
                    mini.set_target(antennas=[0.0, 0.0])
                    await asyncio.sleep(0.15)
            except asyncio.CancelledError:
                mini.set_target(antennas=[0.0, 0.0])

        anim_task = asyncio.create_task(animate_antennas())
        try:
            duration = await tts_engine.speak_to_robot(mini, req.text)
        finally:
            anim_task.cancel()
            try:
                await anim_task
            except asyncio.CancelledError:
                pass
        return {"status": "ok", "duration_secs": duration}
    else:
        # Fallback: antenna animation only (no audio)
        for _ in range(max(1, len(req.text) // 20)):
            mini.set_target(antennas=[0.3, 0.3])
            await asyncio.sleep(0.15)
            mini.set_target(antennas=[0.0, 0.0])
            await asyncio.sleep(0.15)
        return {"status": "ok", "duration_secs": 0.0}


@app.post("/look_at")
async def look_at(req: LookAtRequest):
    """Look at a world-space position."""
    if mini is None:
        raise HTTPException(503, "Robot not connected")
    # Convert world position to head angles (simplified)
    yaw = np.arctan2(req.x, req.z)
    pitch = np.arctan2(-req.y, np.sqrt(req.x**2 + req.z**2))
    head = create_head_pose(roll=0.0, pitch=pitch, yaw=yaw, degrees=False, mm=False)
    mini.goto_target(head=head, duration=0.5)
    return {"status": "ok"}


@app.websocket("/ws/audio")
async def ws_audio(ws: WebSocket):
    """Stream mic audio from Reachy Mini's ReSpeaker array."""
    await audio_ws_handler(ws, mini)


@app.get("/camera/status")
async def camera_status():
    """Check whether the camera is available."""
    if mini is None:
        return {"available": False, "reason": "Robot not connected"}
    try:
        loop = asyncio.get_event_loop()
        frame = await loop.run_in_executor(None, mini.media_manager.get_frame)
        if frame is None:
            return {"available": False, "reason": "No frame returned"}
        return {"available": True, "width": frame.shape[1], "height": frame.shape[0]}
    except Exception as e:
        return {"available": False, "reason": str(e)}


@app.get("/camera/frame")
async def camera_frame(quality: int = 70, max_width: int = 640):
    """Capture a single JPEG frame from the robot's camera."""
    if mini is None:
        raise HTTPException(503, "Robot not connected")

    try:
        import cv2
    except ImportError:
        raise HTTPException(501, "opencv-python-headless not installed")

    loop = asyncio.get_event_loop()
    frame = await loop.run_in_executor(None, mini.media_manager.get_frame)
    if frame is None:
        raise HTTPException(503, "No frame from camera")

    # Resize if wider than max_width
    h, w = frame.shape[:2]
    if w > max_width:
        scale = max_width / w
        new_w = max_width
        new_h = int(h * scale)
        frame = cv2.resize(frame, (new_w, new_h), interpolation=cv2.INTER_AREA)

    # Encode as JPEG
    ok, jpeg = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, quality])
    if not ok:
        raise HTTPException(500, "JPEG encoding failed")

    return Response(content=jpeg.tobytes(), media_type="image/jpeg")


# --- Main ---


def main():
    parser = argparse.ArgumentParser(description="Jiminy Bridge for Sovereign OS")
    parser.add_argument("--port", type=int, default=9100, help="Server port (default: 9100)")
    parser.add_argument("--host", type=str, default="127.0.0.1", help="Bind address")
    parser.add_argument("--sim", action="store_true", help="Force simulation mode")
    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )

    logger.info("Starting Jiminy bridge on %s:%d", args.host, args.port)
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":
    main()
