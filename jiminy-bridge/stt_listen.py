"""Out-of-process speech-to-text for Jiminy.

Runs in the bridge sidecar (NOT the Rust app) because whisper.cpp's ggml and
the app's llama.cpp ggml are merged by /FORCE:MULTIPLE and clash at runtime —
so STT must live outside the app process. Uses faster-whisper (CTranslate2),
which is unrelated to ggml.

Recording goes through the Reachy SDK's `media_manager` (the same path that
streams /ws/audio) rather than raw PortAudio: opening these robot devices
directly via sounddevice returned pure silence, while the SDK captures fine.

Flow (driven by POST /listen): record a turn from the robot mic until ~SILENCE_S
of trailing silence, then transcribe with faster-whisper.
"""
from __future__ import annotations

import logging
import time
from math import gcd
from typing import Optional

import numpy as np
from scipy.signal import resample_poly

logger = logging.getLogger("jiminy.stt")

WHISPER_SAMPLE_RATE = 16000
# faster-whisper model id (downloaded from HF on first use). base.en is fast;
# bump to "small.en" / "distil-large-v3" for more accuracy.
MODEL_NAME = "base.en"
# Energy-based VAD. float32 audio in [-1, 1]; mean-square per chunk.
ENERGY_THRESHOLD = 0.0006
SILENCE_S = 1.2          # trailing silence that ends a turn
MAX_RECORD_S = 15.0      # hard cap on a single turn
START_TIMEOUT_S = 4.0    # give up if no speech starts within this

_model = None


def load_model():
    """Lazily load (and cache) the faster-whisper model. Safe to call eagerly
    from a warmup thread so the first /listen isn't slowed by the download."""
    global _model
    if _model is None:
        from faster_whisper import WhisperModel
        logger.info("Loading faster-whisper '%s' (CPU int8)...", MODEL_NAME)
        _model = WhisperModel(MODEL_NAME, device="cpu", compute_type="int8")
        logger.info("faster-whisper model ready")
    return _model


def record_until_silence(mini) -> Optional[np.ndarray]:
    """Record mono float32 audio via the robot SDK until trailing silence /
    timeout. Returns the recording (resampled to 16 kHz) or None if no speech."""
    mm = mini.media_manager
    try:
        in_rate = int(mm.get_input_audio_samplerate())
    except Exception:
        in_rate = WHISPER_SAMPLE_RATE

    collected: list[np.ndarray] = []
    had_speech = False
    silent_s = 0.0
    elapsed = 0.0
    peak = 0.0

    mm.start_recording()
    try:
        while elapsed < MAX_RECORD_S:
            raw = mm.get_audio_sample()
            if raw is None or len(raw) == 0:
                time.sleep(0.01)
                continue
            samples = np.asarray(raw, dtype=np.float32)
            mono = samples[:, 0] if samples.ndim == 2 else samples.ravel()
            collected.append(mono.copy())
            chunk_s = len(mono) / in_rate
            elapsed += chunk_s
            energy = float(np.mean(mono ** 2))
            peak = max(peak, energy)
            if energy >= ENERGY_THRESHOLD:
                had_speech = True
                silent_s = 0.0
            else:
                silent_s += chunk_s
            if had_speech and silent_s >= SILENCE_S:
                break
            if not had_speech and elapsed >= START_TIMEOUT_S:
                break
    except Exception as e:
        logger.error("STT recording failed: %s", e)
        return None
    finally:
        try:
            mm.stop_recording()
        except Exception:
            pass

    logger.info("STT recorded %.1fs (speech=%s, peak_energy=%.5f, rate=%d)",
                elapsed, had_speech, peak, in_rate)
    if not had_speech or not collected:
        return None

    audio = np.concatenate(collected).astype(np.float32)
    if in_rate != WHISPER_SAMPLE_RATE:
        g = gcd(in_rate, WHISPER_SAMPLE_RATE)
        audio = resample_poly(audio, WHISPER_SAMPLE_RATE // g, in_rate // g).astype(np.float32)
    return audio


def transcribe(audio: Optional[np.ndarray]) -> str:
    if audio is None or len(audio) == 0:
        return ""
    model = load_model()
    segments, _info = model.transcribe(
        audio, language="en", vad_filter=True, beam_size=1)
    return " ".join(s.text.strip() for s in segments).strip()


def listen_and_transcribe(mini) -> str:
    """Blocking record + transcribe; returns recognized text ('' if none).
    Call from a thread/executor — never on the event loop."""
    audio = record_until_silence(mini)
    text = transcribe(audio)
    if text:
        logger.info("STT heard: %s", text[:120])
    else:
        logger.info("STT: nothing recognized")
    return text
