"""Unit tests for the Jiminy sidecar: media-backend resolution + Piper auto-detect.

Run from jiminy-bridge/:  .venv/Scripts/python -m pytest -q
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

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
