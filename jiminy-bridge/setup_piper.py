"""Download the Piper TTS binary + a default voice into jiminy-bridge/piper/.

The bundled binary/voice are gitignored (large, machine-local). Run this once
to enable real speech on the robot's speaker; the sidecar
(tts_engine.create_tts_engine) then auto-detects the result. No env vars needed.

    python setup_piper.py
    python setup_piper.py --voice en/en_GB/alba/medium/en_GB-alba-medium

Voice catalog: https://huggingface.co/rhasspy/piper-voices  (browse the tree).
We pin the classic rhasspy/piper 2023.11.14-2 release because the sidecar drives
the classic CLI (--model/--config/--output-raw).
"""
from __future__ import annotations

import argparse
import io
import os
import platform
import sys
import tarfile
import urllib.request
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEST = os.path.join(HERE, "piper")
VOICES = os.path.join(DEST, "voices")

RELEASE = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/"
ASSETS = {
    ("Windows", "AMD64"): "piper_windows_amd64.zip",
    ("Linux", "x86_64"): "piper_linux_x86_64.tar.gz",
    ("Linux", "aarch64"): "piper_linux_aarch64.tar.gz",
    ("Darwin", "x86_64"): "piper_macos_x64.tar.gz",
    ("Darwin", "arm64"): "piper_macos_aarch64.tar.gz",
}
VOICE_BASE = "https://huggingface.co/rhasspy/piper-voices/resolve/main/"
DEFAULT_VOICE = "en/en_US/amy/medium/en_US-amy-medium"


def _read(url: str) -> bytes:
    print(f"  fetching {url.split('/')[-1].split('?')[0]} ...")
    with urllib.request.urlopen(url) as r:
        return r.read()


def fetch_binary() -> None:
    key = (platform.system(), platform.machine())
    asset = ASSETS.get(key)
    if not asset:
        sys.exit(f"No prebuilt piper for {key}. Grab a binary from {RELEASE} "
                 f"and place piper(.exe) under {DEST}.")
    data = _read(RELEASE + asset)
    if asset.endswith(".zip"):
        zipfile.ZipFile(io.BytesIO(data)).extractall(DEST)
    else:
        tarfile.open(fileobj=io.BytesIO(data), mode="r:gz").extractall(DEST)


def fetch_voice(voice: str) -> None:
    os.makedirs(VOICES, exist_ok=True)
    name = voice.rsplit("/", 1)[-1]
    for ext in (".onnx", ".onnx.json"):
        with open(os.path.join(VOICES, name + ext), "wb") as f:
            f.write(_read(f"{VOICE_BASE}{voice}{ext}?download=true"))


def main() -> None:
    ap = argparse.ArgumentParser(description="Bundle Piper TTS for the Jiminy sidecar")
    ap.add_argument("--voice", default=DEFAULT_VOICE,
                    help=f"piper-voices tree path (default: {DEFAULT_VOICE})")
    args = ap.parse_args()
    os.makedirs(DEST, exist_ok=True)
    print("Downloading Piper binary...")
    fetch_binary()
    print("Downloading voice model...")
    fetch_voice(args.voice)
    print(f"Done. Piper bundled under {DEST} (auto-detected by the sidecar).")


if __name__ == "__main__":
    main()
