"""Unit tests for the Jiminy sidecar: media-backend resolution + Piper auto-detect.

Run from jiminy-bridge/:  .venv/Scripts/python -m pytest -q
"""
import asyncio
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import main  # noqa: E402
import tts_engine  # noqa: E402
from main import resolve_media_backend  # noqa: E402


class TestResolveMediaBackend:
    def test_explicit_flag_wins_over_everything(self):
        assert resolve_media_backend("no_media", True, {"JIMINY_MEDIA_BACKEND": "webrtc"}) == "no_media"

    def test_env_used_when_no_flag(self):
        assert resolve_media_backend(None, True, {"JIMINY_MEDIA_BACKEND": "gstreamer"}) == "gstreamer"

    def test_sim_defaults_to_no_video(self):
        # The --sim fix: headless sim has no camera, so skip video.
        assert resolve_media_backend(None, True, {}) == "default_no_video"

    def test_non_sim_defaults_to_default(self):
        assert resolve_media_backend(None, False, {}) == "default"


class TestStripMarkdownForSpeech:
    s = staticmethod(tts_engine.strip_markdown_for_speech)

    def test_bold_and_italic_unwrapped(self):
        assert self.s("This is **bold** and *italic*.") == "This is bold and italic."

    def test_inline_code_and_links(self):
        assert self.s("Run `make` then see [docs](http://x).") == "Run make then see docs."

    def test_headings_lists_quotes_removed(self):
        assert self.s("# Title\n- one\n- two\n> note") == "Title one two note"

    def test_stray_asterisks_dropped(self):
        assert "*" not in self.s("Use **a** and a lone * here")

    def test_plain_text_unchanged(self):
        assert self.s("Hello Alex, how are you?") == "Hello Alex, how are you?"


def _fake_bundle(tmp_path, monkeypatch):
    (tmp_path / "piper").mkdir()
    binexe = tmp_path / "piper" / "piper.exe"
    binexe.write_bytes(b"")
    voices = tmp_path / "voices"
    voices.mkdir()
    model = voices / "en_US-amy-medium.onnx"
    model.write_bytes(b"")
    (voices / "en_US-amy-medium.onnx.json").write_bytes(b"{}")
    monkeypatch.setattr(tts_engine, "_bundled_piper_dir", lambda: str(tmp_path))
    for var in ("PIPER_BINARY", "PIPER_MODEL", "PIPER_CONFIG"):
        monkeypatch.delenv(var, raising=False)
    return str(binexe), str(model)


class TestPiperAutodetect:
    def test_autodetect_finds_bundle(self, tmp_path, monkeypatch):
        binexe, model = _fake_bundle(tmp_path, monkeypatch)
        assert tts_engine._autodetect_binary() == binexe
        assert tts_engine._autodetect_model() == model

    def test_create_engine_from_bundle(self, tmp_path, monkeypatch):
        binexe, model = _fake_bundle(tmp_path, monkeypatch)
        eng = tts_engine.create_tts_engine()
        assert eng is not None
        assert eng.piper_binary == binexe
        assert eng.model_path == model
        assert eng.config_path == model + ".json"  # auto-derived

    def test_env_overrides_take_priority(self, tmp_path, monkeypatch):
        _fake_bundle(tmp_path, monkeypatch)  # bundle present but env wins
        monkeypatch.setenv("PIPER_BINARY", "my-piper")
        monkeypatch.setenv("PIPER_MODEL", "/voices/custom.onnx")
        eng = tts_engine.create_tts_engine()
        assert eng.piper_binary == "my-piper"
        assert eng.model_path == "/voices/custom.onnx"
        assert eng.config_path == "/voices/custom.onnx.json"

    def test_returns_none_without_a_model(self, tmp_path, monkeypatch):
        monkeypatch.setattr(tts_engine, "_bundled_piper_dir", lambda: str(tmp_path / "empty"))
        for var in ("PIPER_BINARY", "PIPER_MODEL", "PIPER_CONFIG"):
            monkeypatch.delenv(var, raising=False)
        assert tts_engine.create_tts_engine() is None


class TestBargeIn:
    """The /stop (shush) barge-in cancels in-flight speech."""

    def test_cancel_speak_cancels_inflight(self, monkeypatch):
        monkeypatch.setattr(main, "mini", None)  # skip the stop_playing() call
        captured = {}

        async def scenario():
            async def long_speak():
                await asyncio.sleep(10)

            t = asyncio.create_task(long_speak())
            main._speak_task = t
            await asyncio.sleep(0)  # let the task start
            await main._cancel_speak()
            captured["task"] = t

        asyncio.run(scenario())
        assert captured["task"].cancelled()
        assert main._speak_task is None

    def test_cancel_speak_is_noop_when_idle(self, monkeypatch):
        monkeypatch.setattr(main, "mini", None)
        main._speak_task = None
        asyncio.run(main._cancel_speak())
        assert main._speak_task is None

    def test_stop_handler_returns_stopped(self, monkeypatch):
        monkeypatch.setattr(main, "mini", None)
        main._speak_task = None
        assert asyncio.run(main.stop()) == {"status": "stopped"}


class TestCheckAuth:
    """Loopback hardening: browser-origin requests are always rejected, and
    when JIMINY_TOKEN is set every request must carry the bearer token."""

    def test_browser_origin_rejected(self, monkeypatch):
        monkeypatch.setattr(main, "AUTH_TOKEN", "")
        assert main._check_auth("https://evil.example", None) is not None

    def test_tauri_origin_also_rejected_on_bridge(self, monkeypatch):
        # The bridge has no legitimate browser client at all.
        monkeypatch.setattr(main, "AUTH_TOKEN", "")
        assert main._check_auth("http://tauri.localhost", None) is not None

    def test_fails_closed_without_token(self, monkeypatch):
        # SIDECAR-002: no token configured → refuse rather than serve openly.
        monkeypatch.setattr(main, "AUTH_TOKEN", "")
        monkeypatch.setattr(main, "ALLOW_NO_AUTH", False)
        assert main._check_auth(None, None) is not None

    def test_allow_no_auth_opts_into_insecure_dev(self, monkeypatch):
        monkeypatch.setattr(main, "AUTH_TOKEN", "")
        monkeypatch.setattr(main, "ALLOW_NO_AUTH", True)
        assert main._check_auth(None, None) is None
        # Browser-origin requests are still always rejected.
        assert main._check_auth("https://evil.example", None) is not None

    def test_token_required_when_configured(self, monkeypatch):
        monkeypatch.setattr(main, "AUTH_TOKEN", "s3cret")
        assert main._check_auth(None, None) is not None
        assert main._check_auth(None, "Bearer wrong") is not None
        assert main._check_auth(None, "Basic s3cret") is not None
        assert main._check_auth(None, "Bearer s3cret") is None

    def test_origin_rejected_even_with_valid_token(self, monkeypatch):
        monkeypatch.setattr(main, "AUTH_TOKEN", "s3cret")
        assert main._check_auth("https://evil.example", "Bearer s3cret") is not None
