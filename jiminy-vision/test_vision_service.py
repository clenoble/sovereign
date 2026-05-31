"""Unit tests for the Jiminy Vision service: gesture classification, the VLM
window state machine, and the VLM text/image helpers. None of these load
torch or mediapipe (those are imported lazily inside the detector / captioner).

Run from jiminy-vision/:  .venv/Scripts/python -m pytest -q
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from PIL import Image  # noqa: E402

from vision_service import (  # noqa: E402
    GestureDebouncer,
    GestureDetector,
    VisionEngine,
    _extract_answer,
    _resize_max,
)

cg = GestureDetector.classify_gesture  # (index_tip, index_pip, builtin, mouth, others_folded)


class _LM:
    """Minimal stand-in for a MediaPipe landmark (only .y is used here)."""
    def __init__(self, y):
        self.y = y


def _hand(index_ext=True, others_folded=True):
    """Build 21 fake landmarks with the index and the other fingertips posed."""
    lm = [_LM(0.5) for _ in range(21)]
    lm[8], lm[6] = _LM(0.3 if index_ext else 0.7), _LM(0.5)   # index tip/pip
    fold = 0.7 if others_folded else 0.3                       # tip below pip = folded
    lm[12], lm[10] = _LM(fold), _LM(0.5)                       # middle
    lm[16], lm[14] = _LM(fold), _LM(0.5)                       # ring
    lm[20], lm[18] = _LM(fold), _LM(0.5)                       # pinky
    return lm


class TestClassifyGesture:
    def test_shush_when_extended_index_at_mouth(self):
        # index extended (tip.y < pip.y), others folded, tip close to the mouth
        assert cg((0.5, 0.40), (0.5, 0.55), "Pointing_Up", (0.5, 0.42)) == "shush"

    def test_point_when_index_far_from_mouth(self):
        assert cg((0.2, 0.40), (0.2, 0.55), "Pointing_Up", (0.8, 0.9)) == "point"

    def test_no_shush_when_index_not_extended(self):
        # tip below pip -> not extended -> falls back to the built-in category
        assert cg((0.5, 0.6), (0.5, 0.4), "Open_Palm", (0.5, 0.6)) == "open_palm"

    def test_no_shush_without_a_mouth(self):
        assert cg((0.5, 0.40), (0.5, 0.55), "Pointing_Up", None) == "point"

    def test_no_shush_when_other_fingers_extended(self):
        # index at the mouth but the hand is open -> not a quiet sign
        assert cg((0.5, 0.40), (0.5, 0.55), "Open_Palm", (0.5, 0.42),
                  others_folded=False) == "open_palm"

    def test_no_shush_just_beyond_distance_threshold(self):
        # tip 0.11 from the mouth > SHUSH_MAX_DIST(0.10) -> not a shush
        assert cg((0.5, 0.40), (0.5, 0.55), "Pointing_Up", (0.5, 0.51)) == "point"

    def test_shush_just_within_distance_threshold(self):
        # tip 0.09 from the mouth < SHUSH_MAX_DIST(0.10) -> shush
        assert cg((0.5, 0.40), (0.5, 0.55), "Pointing_Up", (0.5, 0.49)) == "shush"


class TestOthersFolded:
    def test_true_when_only_index_extended(self):
        assert GestureDetector._others_folded(_hand(others_folded=True)) is True

    def test_false_when_a_finger_is_extended(self):
        assert GestureDetector._others_folded(_hand(others_folded=False)) is False

    def test_builtin_mapping(self):
        assert cg((0, 0), (0, 1), "Open_Palm", None) == "open_palm"
        assert cg((0, 0), (0, 1), "Closed_Fist", None) == "fist"
        assert cg((0, 0), (0, 1), "Thumb_Up", None) == "thumbs_up"
        assert cg((0, 0), (0, 1), "Victory", None) == "victory"

    def test_unknown_or_none_builtin_returns_none(self):
        assert cg((0, 0), (0, 1), "None", None) is None
        assert cg((0, 0), (0, 1), None, None) is None


class TestGestureDebouncer:
    def test_single_frame_false_positive_is_suppressed(self):
        # One stray "shush" frame surrounded by None must never commit.
        d = GestureDebouncer(stable_frames=3)
        assert d.update(None) is None
        assert d.update("shush") is None   # 1 frame — not yet stable
        assert d.update(None) is None      # gone again — still None committed
        assert d.committed is None

    def test_sustained_gesture_commits_after_threshold(self):
        d = GestureDebouncer(stable_frames=3)
        assert d.update("shush") is None   # 1
        assert d.update("shush") is None   # 2
        assert d.update("shush") == "shush"  # 3 -> committed
        assert d.update("shush") == "shush"  # stays committed while held

    def test_release_clears_the_latch(self):
        # A held sign that goes away must clear (no permanent latch).
        d = GestureDebouncer(stable_frames=2)
        d.update("shush"); d.update("shush")
        assert d.committed == "shush"
        assert d.update(None) == "shush"   # 1 None frame — not yet stable
        assert d.update(None) is None      # 2 None frames -> cleared
        assert d.committed is None

    def test_flicker_resets_the_streak(self):
        # shush, point, shush, shush, shush -> only the final run counts.
        d = GestureDebouncer(stable_frames=3)
        assert d.update("shush") is None
        assert d.update("point") is None   # different -> streak resets
        assert d.update("shush") is None   # streak 1
        assert d.update("shush") is None   # streak 2
        assert d.update("shush") == "shush"  # streak 3 -> commit

    def test_threshold_of_one_commits_immediately(self):
        d = GestureDebouncer(stable_frames=1)
        assert d.update("open_palm") == "open_palm"


class FakeClock:
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        return self.t

    def advance(self, dt):
        self.t += dt


def _engine(clock, window=300.0):
    # window logic never touches source/detector/vlm, so None is fine here.
    return VisionEngine(source=None, detector=None, vlm=None,
                        window_seconds=window, vlm_interval=4.0, clock=clock)


class TestVisionWindow:
    def test_inactive_by_default(self):
        e = _engine(FakeClock())
        assert e.window_active() is False
        assert e.snapshot()["window_active"] is False

    def test_open_activates_for_default_duration(self):
        e = _engine(FakeClock())
        e.open_window()
        assert e.window_active() is True
        snap = e.snapshot()
        assert snap["window_active"] is True
        assert 299.0 <= snap["window_remaining_s"] <= 300.0

    def test_window_expires_after_its_duration(self):
        clk = FakeClock()
        e = _engine(clk)
        e.open_window(10.0)
        clk.advance(9)
        assert e.window_active() is True
        clk.advance(2)  # now 11s elapsed > 10s
        assert e.window_active() is False
        assert e.snapshot()["window_remaining_s"] == 0.0

    def test_close_deactivates_immediately(self):
        e = _engine(FakeClock())
        e.open_window(100.0)
        e.close_window()
        assert e.window_active() is False


class TestVlmHelpers:
    def test_extract_answer_strips_the_prompt_turn(self):
        assert _extract_answer("User: hi\nAssistant: a bee on a flower") == "a bee on a flower"

    def test_extract_answer_passthrough_without_marker(self):
        assert _extract_answer("  a plain caption  ") == "a plain caption"

    def test_resize_max_downscales_preserving_aspect(self):
        out = _resize_max(Image.new("RGB", (2000, 1000)), 768)
        assert out.size == (768, 384)

    def test_resize_max_keeps_small_images(self):
        assert _resize_max(Image.new("RGB", (100, 50)), 768).size == (100, 50)
