#!/usr/bin/env python3
"""Sanity-check the WAVs written by `xmas-tui --export-audio DIR`.

For each file: duration, peak (dBFS), RMS over the audible part (dBFS),
loudest 400 ms window (dBFS), DC offset, NaN/inf and clipped samples.
Flags anything that would sound wrong: clipping, peaks above -1 dBFS,
DC offset, silence, non-finite samples. Exit status 1 if anything failed.

    python3 scripts/check-audio.py /tmp/aud
"""
import math
import struct
import sys
from pathlib import Path


def read_wav(path):
    """Minimal RIFF reader: PCM16 or IEEE float32, any channel count."""
    data = path.read_bytes()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("not a WAV file")
    pos, fmt, rate, ch, bits, frames = 12, None, 0, 0, 0, None
    while pos + 8 <= len(data):
        cid, size = data[pos:pos + 4], struct.unpack("<I", data[pos + 4:pos + 8])[0]
        body = data[pos + 8:pos + 8 + size]
        if cid == b"fmt ":
            fmt, ch, rate = struct.unpack("<HHI", body[:8])
            bits = struct.unpack("<H", body[14:16])[0]
        elif cid == b"data":
            frames = body
        pos += 8 + size + (size & 1)
    if fmt == 3 and bits == 32:
        n = len(frames) // 4
        samples = struct.unpack(f"<{n}f", frames[:n * 4])
    elif fmt == 1 and bits == 16:
        n = len(frames) // 2
        samples = [s / 32768.0 for s in struct.unpack(f"<{n}h", frames[:n * 2])]
    else:
        raise ValueError(f"unsupported format {fmt}/{bits}")
    return rate, ch, samples


def db(x):
    return 20 * math.log10(x) if x > 0 else -math.inf


def analyse(path):
    rate, ch, s = read_wav(path)
    try:
        import numpy as np
        a = np.asarray(s, dtype=np.float64)
        bad = int((~np.isfinite(a)).sum())
        a = np.where(np.isfinite(a), a, 0.0)
        peak = float(np.abs(a).max()) if a.size else 0.0
        clipped = int((np.abs(a) >= 0.999).sum())
        dc = float(a.mean()) if a.size else 0.0
        mono = a.reshape(-1, ch).mean(axis=1) if ch > 1 else a
        win = max(1, int(rate * 0.4))
        n = len(mono) // win
        rms_w = np.sqrt((mono[: n * win].reshape(n, win) ** 2).mean(axis=1)) if n else np.array([0.0])
        audible = rms_w[rms_w > 10 ** (-70 / 20)]
        rms = float(np.sqrt((audible ** 2).mean())) if audible.size else 0.0
        loudest = float(rms_w.max())
    except ImportError:  # plain-Python fallback
        bad = sum(1 for x in s if not math.isfinite(x))
        s = [x if math.isfinite(x) else 0.0 for x in s]
        peak = max((abs(x) for x in s), default=0.0)
        clipped = sum(1 for x in s if abs(x) >= 0.999)
        dc = sum(s) / max(1, len(s))
        rms = math.sqrt(sum(x * x for x in s) / max(1, len(s)))
        loudest = rms
    return {
        "dur": len(s) / ch / rate,
        "peak": db(peak),
        "rms": db(rms),
        "loud": db(loudest),
        "dc": dc,
        "bad": bad,
        "clip": clipped,
    }


def main():
    d = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    files = sorted(d.glob("*.wav"))
    if not files:
        sys.exit(f"no .wav files in {d}")
    print(f"{'file':34} {'dur':>6} {'peak':>7} {'rms':>7} {'loud':>7} {'dc':>8} {'nan':>4} {'clip':>5}  verdict")
    failed = 0
    for f in files:
        r = analyse(f)
        problems = []
        if r["bad"]:
            problems.append("NaN/inf")
        if r["clip"]:
            problems.append("clipping")
        if r["peak"] > -1.0:
            problems.append("peak > -1 dBFS")
        if abs(r["dc"]) > 0.002:
            problems.append("DC offset")
        if r["peak"] < -60:
            problems.append("silent")
        verdict = "ok" if not problems else "FAIL: " + ", ".join(problems)
        failed += bool(problems)
        print(f"{f.stem:34} {r['dur']:6.1f} {r['peak']:7.1f} {r['rms']:7.1f} {r['loud']:7.1f} {r['dc']:8.5f} {r['bad']:4d} {r['clip']:5d}  {verdict}")
    print(f"\n{len(files)} files, {failed} with problems  (dBFS; rms = audible part, loud = loudest 400 ms)")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
