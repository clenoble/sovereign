"""Piper TTS engine for Jiminy — generates speech and pushes to Reachy Mini speaker.

Piper produces raw PCM at 22050 Hz, S16_LE, mono. The Reachy Mini speaker
expects 16kHz stereo float32. This module handles the conversion and streaming.
"""

from __future__ import annotations

import asyncio
import logging
import os
from math import gcd
from typing import Optional

import numpy as np
from scipy.signal import resample_poly

from reachy_mini import ReachyMini

logger = logging.getLogger("jiminy.tts")

PIPER_SAMPLE_RATE = 22050
REACHY_SAMPLE_RATE = 16000
# Piper phoneme-duration scale: <1.0 speaks faster, >1.0 slower (default 1.0).
# The en_US-amy-medium voice reads a touch slowly at 1.0.
PIPER_LENGTH_SCALE = 0.9


class PiperTts:
    """Wraps the Piper TTS binary and streams output to Reachy Mini's speaker."""

    def __init__(self, piper_binary: str, model_path: str, config_path: str) -> None:
        self.piper_binary = piper_binary
        self.model_path = model_path
        self.config_path = config_path

    async def speak_to_robot(self, mini: ReachyMini, text: str) -> float:
        """Run Piper TTS and push audio to the robot's speaker.

        Returns the duration in seconds (0.0 if no audio was generated).
        """
        if not text.strip():
            return 0.0

        pcm_data = await self._run_piper(text)
        if not pcm_data:
            logger.warning("Piper produced no audio for: %s", text[:60])
            return 0.0

        # Decode S16_LE mono to float32 [-1, 1]
        samples_mono = np.frombuffer(pcm_data, dtype=np.int16).astype(np.float32) / 32768.0

        # Resample Piper's 22050 Hz to the robot speaker's ACTUAL output rate.
        # The SoundDevice backend reports the output device's native rate
        # (usually 44100/48000) from get_output_audio_samplerate() — resampling
        # to a fixed 16000 made the speech play ~3x fast and high-pitched.
        out_rate = int(mini.media_manager.get_output_audio_samplerate())
        logger.info("TTS: resampling 22050 -> %d Hz (robot output rate)", out_rate)
        if out_rate != PIPER_SAMPLE_RATE:
            g = gcd(out_rate, PIPER_SAMPLE_RATE)
            resampled = resample_poly(
                samples_mono, up=out_rate // g, down=PIPER_SAMPLE_RATE // g
            ).astype(np.float32)
        else:
            resampled = samples_mono.astype(np.float32)

        # Get output channel count from robot
        out_channels = mini.media_manager.get_output_channels()

        # Mono -> stereo (or match whatever the robot expects)
        if out_channels >= 2:
            stereo = np.column_stack([resampled] * out_channels).astype(np.float32)
        else:
            stereo = resampled.reshape(-1, 1).astype(np.float32)

        duration = len(resampled) / out_rate

        # Push to speaker in chunks (~100ms each). Pace ~15% ahead of real-time
        # so the device buffer never underruns.
        chunk_size = max(1, out_rate // 10)
        pace = 0.85
        mini.media_manager.start_playing()
        try:
            for i in range(0, len(stereo), chunk_size):
                chunk = stereo[i : i + chunk_size]
                mini.media_manager.push_audio_sample(chunk)
                await asyncio.sleep(chunk_size / out_rate * pace)
            # Pacing ahead means ~(1-pace) of the audio is still buffered when the
            # loop ends; drain it (plus device latency) before stop_playing() or
            # the tail of the sentence gets clipped. A barge-in /stop cancels this
            # task mid-loop, so it jumps straight to the finally (no drain) — good.
            await asyncio.sleep(duration * (1.0 - pace) + 0.3)
        finally:
            mini.media_manager.stop_playing()

        logger.info("Spoke %d chars in %.1fs", len(text), duration)
        return duration

    async def _run_piper(self, text: str) -> bytes:
        """Run the Piper subprocess and return raw PCM bytes."""
        cmd = [
            self.piper_binary,
            "--model", self.model_path,
            "--config", self.config_path,
            "--length_scale", str(PIPER_LENGTH_SCALE),
            "--output-raw",
        ]
        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            stdout, stderr = await proc.communicate(text.encode("utf-8"))
            if proc.returncode != 0:
                logger.error("Piper failed (rc=%d): %s", proc.returncode, stderr.decode()[:200])
                return b""
            return stdout
        except FileNotFoundError:
            logger.error("Piper binary not found: %s", self.piper_binary)
            return b""


def _bundled_piper_dir() -> str:
    """The local jiminy-bridge/piper/ dir where the binary + voices are bundled."""
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), "piper")


def _autodetect_binary() -> Optional[str]:
    base = _bundled_piper_dir()
    candidates = [
        os.path.join(base, "piper", "piper.exe"),  # Windows zip layout
        os.path.join(base, "piper.exe"),
        os.path.join(base, "piper", "piper"),       # Linux / macOS layout
        os.path.join(base, "piper"),
    ]
    return next((c for c in candidates if os.path.isfile(c)), None)


def _autodetect_model() -> Optional[str]:
    """First *.onnx under piper/ that has a sibling *.onnx.json config."""
    base = _bundled_piper_dir()
    if not os.path.isdir(base):
        return None
    for root, _dirs, files in os.walk(base):
        for f in sorted(files):
            if f.endswith(".onnx") and os.path.isfile(os.path.join(root, f + ".json")):
                return os.path.join(root, f)
    return None


def create_tts_engine() -> Optional[PiperTts]:
    """Create a PiperTts, preferring env vars then a bundled piper/ dir.

    Resolution for binary / model / config:
      1. PIPER_BINARY / PIPER_MODEL / PIPER_CONFIG env vars
      2. auto-detected binary + first voice under jiminy-bridge/piper/
    Returns None (→ /speak antenna-only fallback) if no model can be found.
    """
    piper_bin = os.environ.get("PIPER_BINARY") or _autodetect_binary() or "piper"
    piper_model = os.environ.get("PIPER_MODEL") or _autodetect_model() or ""
    piper_config = os.environ.get("PIPER_CONFIG") or (
        piper_model + ".json" if piper_model else "")

    if not piper_model:
        logger.info(
            "TTS not configured (no PIPER_MODEL env and no bundled voice under %s). "
            "/speak will use fallback animation.", _bundled_piper_dir())
        return None

    engine = PiperTts(piper_bin, piper_model, piper_config)
    logger.info("Piper TTS initialized: binary=%s model=%s", piper_bin, piper_model)
    return engine
