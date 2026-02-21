"""Emotion and dance library for Jiminy (Reachy Mini).

Loads pre-recorded animations from Hugging Face datasets and provides
playback through the Reachy Mini SDK.
"""

from __future__ import annotations

import logging
from typing import Optional

from reachy_mini import ReachyMini
from reachy_mini.motion.recorded_move import RecordedMove, RecordedMoves

logger = logging.getLogger("jiminy.emotions")

EMOTIONS_DATASET = "pollen-robotics/reachy-mini-emotions-library"
DANCES_DATASET = "pollen-robotics/reachy-mini-dances-library"


class EmotionLibrary:
    """Manages emotion and dance animations for Reachy Mini."""

    def __init__(self) -> None:
        self._emotions: Optional[RecordedMoves] = None
        self._dances: Optional[RecordedMoves] = None

    def load(self) -> None:
        """Load emotion and dance datasets from Hugging Face."""
        logger.info("Loading emotions from %s", EMOTIONS_DATASET)
        self._emotions = RecordedMoves(EMOTIONS_DATASET)
        logger.info(
            "Loaded %d emotions: %s",
            len(self.list_emotions()),
            ", ".join(self.list_emotions()[:5]),
        )

        logger.info("Loading dances from %s", DANCES_DATASET)
        self._dances = RecordedMoves(DANCES_DATASET)
        logger.info(
            "Loaded %d dances: %s",
            len(self.list_dances()),
            ", ".join(self.list_dances()[:5]),
        )

    def list_emotions(self) -> list[str]:
        if self._emotions is None:
            return []
        return self._emotions.list_moves()

    def list_dances(self) -> list[str]:
        if self._dances is None:
            return []
        return self._dances.list_moves()

    def get_emotion(self, name: str) -> Optional[RecordedMove]:
        if self._emotions is None:
            return None
        try:
            return self._emotions.get(name)
        except (KeyError, IndexError):
            logger.warning("Emotion '%s' not found", name)
            return None

    def get_dance(self, name: str) -> Optional[RecordedMove]:
        if self._dances is None:
            return None
        try:
            return self._dances.get(name)
        except (KeyError, IndexError):
            logger.warning("Dance '%s' not found", name)
            return None

    def play_emotion(self, mini: ReachyMini, name: str) -> bool:
        """Play a named emotion animation. Returns True if played."""
        move = self.get_emotion(name)
        if move is None:
            return False
        logger.debug("Playing emotion: %s", name)
        mini.play_move(move, initial_goto_duration=0.5)
        return True

    def play_dance(self, mini: ReachyMini, name: str) -> bool:
        """Play a named dance animation. Returns True if played."""
        move = self.get_dance(name)
        if move is None:
            return False
        logger.debug("Playing dance: %s", name)
        mini.play_move(move, initial_goto_duration=1.0)
        return True
