#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regenerate the original, synthetic MP3s for the local transition diagnostic."""

import argparse
import math
import struct
import subprocess
import tempfile
import wave
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path,
        default=Path(__file__).resolve().parents[1] / "resources" / "playback-fixtures",
    )
    output = parser.parse_args().output
    output.mkdir(parents=True, exist_ok=True)
    rate = 44_100
    with tempfile.TemporaryDirectory(prefix="karin-transition-fixtures-") as directory:
        original = Path(directory) / "original.wav"
        for name, frame_count, frequency in [("a", 12_000, 443), ("b", 15_600, 659)]:
            samples = struct.pack(
                f"<{frame_count}h",
                *(int(5192 * math.sin(2 * math.pi * frequency * frame / rate))
                  for frame in range(frame_count)),
            )
            with wave.open(str(original), "wb") as wav:
                wav.setnchannels(1)
                wav.setsampwidth(2)
                wav.setframerate(rate)
                wav.writeframes(samples)
            subprocess.run(
                ["lame", "--silent", "--noreplaygain", "-m", "m", "-b", "64",
                 str(original), str(output / f"transition-{name}.mp3")],
                check=True,
            )


if __name__ == "__main__":
    main()
