"""WebSocket audio streaming from Reachy Mini's ReSpeaker 4-mic array.

Streams f32 LE PCM at 16 kHz mono to a connected Rust client.  Also
supports direction-of-arrival (DoA) queries.

Protocol:
    Client → Server (JSON text):
        {"cmd": "start"}     — begin streaming mic audio
        {"cmd": "stop"}      — stop streaming
        {"cmd": "doa"}       — request current direction-of-arrival

    Server → Client:
        binary frames        — f32 LE PCM (16 kHz mono)
        JSON text            — {"type":"doa","angle_rad":1.2,"speech_detected":true}
"""

from __future__ import annotations

import asyncio
import json
import logging
from typing import Optional

import numpy as np
from fastapi import WebSocket, WebSocketDisconnect

from reachy_mini import ReachyMini

logger = logging.getLogger("jiminy.audio")

REACHY_SAMPLE_RATE = 16000


async def audio_ws_handler(ws: WebSocket, mini: Optional[ReachyMini]) -> None:
    """Handle a single WebSocket audio session."""
    await ws.accept()

    if mini is None:
        await ws.send_json({"type": "error", "msg": "Robot not connected"})
        await ws.close(1011, "Robot not connected")
        return

    streaming = False
    stream_task: Optional[asyncio.Task] = None

    async def stream_audio() -> None:
        """Continuously read mic samples and forward as binary f32 LE."""
        nonlocal streaming
        in_channels = mini.media_manager.get_input_channels()
        logger.info("Mic streaming started (channels=%d)", in_channels)
        mini.media_manager.start_recording()
        try:
            while streaming:
                # get_audio_sample() returns ndarray (samples, channels) float32
                # Block briefly in executor to avoid holding the event loop
                loop = asyncio.get_event_loop()
                try:
                    raw = await asyncio.wait_for(
                        loop.run_in_executor(
                            None, mini.media_manager.get_audio_sample
                        ),
                        timeout=1.0,
                    )
                except asyncio.TimeoutError:
                    continue

                if raw is None or len(raw) == 0:
                    await asyncio.sleep(0.01)
                    continue

                samples = np.asarray(raw, dtype=np.float32)

                # Downmix to mono if multi-channel
                if samples.ndim == 2 and samples.shape[1] > 1:
                    mono = samples[:, 0]  # Take first channel
                else:
                    mono = samples.ravel()

                # Send as raw f32 LE bytes
                await ws.send_bytes(mono.tobytes())
        except (WebSocketDisconnect, asyncio.CancelledError):
            pass
        except Exception as e:
            logger.error("Audio stream error: %s", e)
        finally:
            mini.media_manager.stop_recording()
            logger.info("Mic streaming stopped")

    try:
        while True:
            msg = await ws.receive_text()
            try:
                data = json.loads(msg)
            except json.JSONDecodeError:
                continue

            cmd = data.get("cmd", "")

            if cmd == "start" and not streaming:
                streaming = True
                stream_task = asyncio.create_task(stream_audio())
                await ws.send_json({"type": "status", "streaming": True})

            elif cmd == "stop" and streaming:
                streaming = False
                if stream_task is not None:
                    stream_task.cancel()
                    try:
                        await stream_task
                    except asyncio.CancelledError:
                        pass
                    stream_task = None
                await ws.send_json({"type": "status", "streaming": False})

            elif cmd == "doa":
                try:
                    loop = asyncio.get_event_loop()
                    doa = await loop.run_in_executor(
                        None, mini.media_manager.get_doa
                    )
                    await ws.send_json({
                        "type": "doa",
                        "angle_rad": float(doa.get("angle", 0.0)),
                        "speech_detected": bool(doa.get("speech", False)),
                    })
                except Exception as e:
                    await ws.send_json({
                        "type": "error",
                        "msg": f"DoA unavailable: {e}",
                    })

    except WebSocketDisconnect:
        logger.info("Audio WebSocket client disconnected")
    finally:
        streaming = False
        if stream_task is not None:
            stream_task.cancel()
            try:
                await stream_task
            except asyncio.CancelledError:
                pass
