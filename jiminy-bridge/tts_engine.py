"""Piper TTS engine for Jiminy — generates speech and pushes to Reachy Mini speaker.

Piper produces raw PCM at 22050 Hz, S16_LE, mono. The Reachy Mini speaker
expects 16kHz stereo float32. This module handles the conversion and streaming.
"""

from __future__ import annotations

import asyncio
import logging
import os
from typing import Optional

import numpy as np
from scipy.signal import resample_poly

from reachy_mini import ReachyMini

logger = logging.getLogger("jiminy.tts")

PIPER_SAMPLE_RATE = 22050
REACHY_SAMPLE_RATE = 16000


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

        # Resample 22050 -> 16000 Hz using polyphase filter (better than interp)
        # GCD(22050, 16000) = 50 → up=320, down=441
        resampled = resample_poly(samples_mono, up=320, down=441).astype(np.float32)

        # Get output channel count from robot
        out_channels = mini.media_manager.get_output_channels()

        # Mono -> stereo (or match whatever the robot expects)
        if out_channels >= 2:
            stereo = np.column_stack([resampled] * out_channels).astype(np.float32)
        else:
            stereo = resampled.reshape(-1, 1).astype(np.float32)

        duration = len(resampled) / REACHY_SAMPLE_RATE

        # Push to speaker in chunks (~100ms each)
        chunk_size = REACHY_SAMPLE_RATE // 10  # 1600 samples = 100ms
        mini.media_manager.start_playing()
        try:
            for i in range(0, len(stereo), chunk_size):
                chunk = stereo[i : i + chunk_size]
                mini.media_manager.push_audio_sample(chunk)
                # Pace slightly ahead to avoid underrun
                await asyncio.sleep(chunk_size / REACHY_SAMPLE_RATE * 0.85)
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


def create_tts_engine() -> Optional[PiperTts]:
    """Create a PiperTts from environment variables (returns None if not configured)."""
    piper_bin = os.environ.get("PIPER_BINARY", "piper")
    piper_model = os.environ.get("PIPER_MODEL", "")
    piper_config = os.environ.get("PIPER_CONFIG", "")

    if not piper_model:
        logger.info("TTS not configured (set PIPER_MODEL env). /speak will use fallback animation.")
        return None

    engine = PiperTts(piper_bin, piper_model, piper_config)
    logger.info("Piper TTS initialized: model=%s", piper_model)
    return engine
