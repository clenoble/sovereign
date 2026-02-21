"""Jiminy Bridge — FastAPI sidecar connecting Sovereign OS to Reachy Mini.

Sovereign OS (Rust) sends HTTP commands here, and this server translates
them into Reachy Mini SDK calls. Supports both simulation (MuJoCo) and
real hardware.

Usage:
    pip install -r requirements.txt
    python main.py                    # Auto-detect (sim or hardware)
    python main.py --sim              # Force simulation mode
    python main.py --port 9100        # Custom port
"""

from __future__ import annotations

import argparse
import asyncio
import logging
from contextlib import asynccontextmanager
from typing import Optional

import numpy as np
import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

from reachy_mini import ReachyMini
from reachy_mini.utils import create_head_pose

from emotions import EmotionLibrary

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
_idle_task: Optional[asyncio.Task] = None


# --- Lifespan ---


@asynccontextmanager
async def lifespan(app: FastAPI):
    global mini, library
    logger.info("Starting Jiminy bridge...")

    # Connect to robot (auto-detects sim or hardware)
    try:
        mini = ReachyMini(media_backend="no_media")
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
    played = library.play_emotion(mini, req.name)
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
    played = library.play_dance(mini, req.name)
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
    """Speak text (placeholder — requires TTS integration)."""
    if mini is None:
        raise HTTPException(503, "Robot not connected")
    logger.info("Speaking: %s", req.text[:80])
    # TODO: Integrate with robot's speaker or system TTS
    # For now, just animate the antennas to simulate speech
    for _ in range(3):
        mini.set_target(antennas=[0.3, 0.3])
        await asyncio.sleep(0.15)
        mini.set_target(antennas=[0.0, 0.0])
        await asyncio.sleep(0.15)
    return {"status": "ok", "text_length": len(req.text)}


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
