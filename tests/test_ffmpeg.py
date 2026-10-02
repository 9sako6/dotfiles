"""Opt-in, offline behavior checks for the pinned FFmpeg distribution.

Run once for each distribution, using its real executables (not PATH shims):
    DOTFILES_TEST_FFMPEG=/path/to/bin/ffmpeg \
    DOTFILES_TEST_FFPROBE=/path/to/bin/ffprobe \
    python3 -m unittest discover -s tests -p test_ffmpeg.py -v

DOTFILES_TEST_FFPLAY optionally selects ffplay. Otherwise, a sibling of ffmpeg
is checked when present. Only its version is run; no display or audio device is
opened. With no binary paths configured, unittest discovery skips this suite.
"""

from fractions import Fraction
import json
import math
import os
from pathlib import Path
import re
import shlex
import struct
import subprocess
import sys
import tempfile
import unittest
import wave


EXPECTED_VERSION = "8.1.2"
WIDTH, HEIGHT, FRAME_RATE, FRAME_COUNT = 96, 64, 12, 12
SAMPLE_RATE = 48000
DURATION = FRAME_COUNT / FRAME_RATE
TITLE = "dotfiles FFmpeg behavior fixture"


def run(*arguments):
    command = [str(argument) for argument in arguments]
    try:
        result = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=30,
            env=dict(os.environ, AV_LOG_FORCE_NOCOLOR="1", LC_ALL="C"),
        )
    except subprocess.TimeoutExpired as error:
        raise AssertionError(
            f"Command timed out: {shlex.join(command)}\n"
            f"stdout:\n{(error.stdout or b'').decode(errors='replace')}\n"
            f"stderr:\n{(error.stderr or b'').decode(errors='replace')}"
        ) from error
    except OSError as error:
        raise AssertionError(f"Could not run {shlex.join(command)}: {error}") from error
    if result.returncode:
        raise AssertionError(
            f"Command failed ({result.returncode}): {shlex.join(command)}\n"
            f"stdout:\n{result.stdout.decode(errors='replace')}\n"
            f"stderr:\n{result.stderr.decode(errors='replace')}"
        )
    return result.stdout


def binary_path(value, variable):
    path = Path(value).expanduser()
    if not path.is_absolute() or not path.is_file() or not os.access(path, os.X_OK):
        raise AssertionError(f"{variable} must name an absolute executable path: {value!r}")
    return path


def check_version(binary, tool):
    output = run(binary, "-version").decode(errors="replace")
    print(f"\n{binary}\n{output}", file=sys.stderr)
    match = re.match(rf"{tool} version (\S+)", output)
    actual = match.group(1) if match else None
    # Distribution suffixes are allowed, but a different upstream release is not.
    if actual is None or actual.split("-", 1)[0] != EXPECTED_VERSION:
        raise AssertionError(f"Expected {tool} {EXPECTED_VERSION}, found {actual!r}: {binary}")


class FFmpegBehaviorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        variables = ("DOTFILES_TEST_FFMPEG", "DOTFILES_TEST_FFPROBE", "DOTFILES_TEST_FFPLAY")
        if not any(os.environ.get(variable) for variable in variables):
            raise unittest.SkipTest("set DOTFILES_TEST_FFMPEG and DOTFILES_TEST_FFPROBE to real binaries")
        for variable in variables[:2]:
            if not os.environ.get(variable):
                raise AssertionError(f"{variable} is required when opting into FFmpeg behavior tests")
        cls.ffmpeg = binary_path(os.environ[variables[0]], variables[0])
        cls.ffprobe = binary_path(os.environ[variables[1]], variables[1])
        check_version(cls.ffmpeg, "ffmpeg")
        check_version(cls.ffprobe, "ffprobe")

        cls.ffplay = None
        if os.environ.get(variables[2]):
            cls.ffplay = binary_path(os.environ[variables[2]], variables[2])
        elif (cls.ffmpeg.parent / "ffplay").exists():
            cls.ffplay = binary_path(str(cls.ffmpeg.parent / "ffplay"), variables[2])

        temporary = tempfile.TemporaryDirectory(prefix="dotfiles-ffmpeg-")
        cls.addClassCleanup(temporary.cleanup)
        cls.root = Path(temporary.name)
        cls.media = cls.root / "converted.mp4"
        video = cls.root / "synthetic.yuv"
        audio = cls.root / "synthetic.wav"

        # Deterministic raw YUV and PCM inputs require no optional source filters.
        with video.open("wb") as output:
            for frame in range(FRAME_COUNT):
                output.write(bytes(
                    200 if x < WIDTH // 2 and y < HEIGHT // 2 else 40 + frame * 12
                    for y in range(HEIGHT) for x in range(WIDTH)
                ))
                output.write(bytes([128]) * (WIDTH * HEIGHT // 2))
        cls.samples = [
            round(10000 * math.sin(2 * math.pi * 440 * index / SAMPLE_RATE))
            for index in range(round(SAMPLE_RATE * DURATION))
        ]
        with wave.open(str(audio), "wb") as output:
            output.setparams((1, 2, SAMPLE_RATE, len(cls.samples), "NONE", "not compressed"))
            output.writeframes(struct.pack(f"<{len(cls.samples)}h", *cls.samples))

        cls.convert(
            "-f", "rawvideo", "-pixel_format", "yuv420p", "-video_size", f"{WIDTH}x{HEIGHT}",
            "-framerate", FRAME_RATE, "-i", video, "-i", audio,
            "-map", "0:v:0", "-map", "1:a:0", "-t", DURATION,
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "18", "-threads:v", "1",
            "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k",
            "-metadata", f"title={TITLE}", "-movflags", "+faststart", cls.media,
        )

    @classmethod
    def convert(cls, *arguments):
        return run(cls.ffmpeg, "-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-xerror", *arguments)

    @classmethod
    def probe(cls, media, *arguments):
        output = run(cls.ffprobe, "-v", "error", "-of", "json", *arguments, media)
        try:
            return json.loads(output)
        except json.JSONDecodeError as error:
            raise AssertionError(f"Invalid ffprobe JSON for {media}: {output!r}") from error

    def test_h264_aac_conversion_and_probe_metadata(self):
        metadata = self.probe(self.media, "-show_streams", "-show_format", "-count_frames")
        self.assertEqual(len(metadata["streams"]), 2, metadata)
        streams = {stream["codec_type"]: stream for stream in metadata["streams"]}
        video, audio = streams["video"], streams["audio"]
        self.assertEqual(video["codec_name"], "h264", video)
        self.assertEqual((video["width"], video["height"]), (WIDTH, HEIGHT), video)
        self.assertEqual(video["pix_fmt"], "yuv420p", video)
        self.assertEqual(Fraction(video["avg_frame_rate"]), FRAME_RATE, video)
        self.assertEqual(int(video["nb_read_frames"]), FRAME_COUNT, video)
        self.assertEqual(audio["codec_name"], "aac", audio)
        self.assertEqual((int(audio["sample_rate"]), audio["channels"]), (SAMPLE_RATE, 1), audio)
        self.assertGreater(int(audio["nb_read_frames"]), 0, audio)
        for section in (video, audio, metadata["format"]):
            self.assertAlmostEqual(float(section["duration"]), DURATION, delta=0.05, msg=section)
        self.assertEqual(metadata["format"]["tags"]["title"], TITLE, metadata)

    def test_complete_audio_and_video_decode(self):
        progress = self.convert(
            "-err_detect", "explode", "-i", self.media, "-map", "0:v:0", "-map", "0:a:0",
            "-progress", "pipe:1", "-nostats", "-f", "null", "-",
        ).decode()
        final = dict(line.split("=", 1) for line in progress.splitlines() if "=" in line)
        self.assertEqual(final.get("progress"), "end", progress)
        self.assertEqual(int(final["frame"]), FRAME_COUNT, progress)
        self.assertGreaterEqual(int(final["out_time_us"]), round(DURATION * 1_000_000), progress)

    def test_audio_extraction_preserves_the_signal(self):
        extracted = self.root / "extracted.wav"
        self.convert(
            "-i", self.media, "-map", "0:a:0", "-vn", "-t", DURATION,
            "-c:a", "pcm_s16le", "-ar", SAMPLE_RATE, "-ac", "1", extracted,
        )
        with wave.open(str(extracted), "rb") as audio:
            self.assertEqual((audio.getnchannels(), audio.getsampwidth(), audio.getframerate()), (1, 2, SAMPLE_RATE))
            self.assertEqual(audio.getnframes(), len(self.samples))
            samples = struct.unpack(f"<{audio.getnframes()}h", audio.readframes(audio.getnframes()))
        energy = sum(sample * sample for sample in samples)
        reference_energy = sum(sample * sample for sample in self.samples)
        self.assertGreater(energy, 0, "PCM extraction is silent")
        self.assertAlmostEqual(math.sqrt(energy / reference_energy), 1, delta=0.15)
        correlation = sum(left * right for left, right in zip(samples, self.samples)) / math.sqrt(energy * reference_energy)
        self.assertGreater(correlation, 0.98, "Decoded AAC does not match the synthetic 440 Hz input")

    def test_png_frame_extraction_preserves_pixels(self):
        image = self.root / "frame.png"
        self.convert("-i", self.media, "-map", "0:v:0", "-frames:v", "1", "-c:v", "png", "-update", "1", image)
        metadata = self.probe(image, "-show_streams")["streams"][0]
        self.assertEqual((metadata["codec_name"], metadata["width"], metadata["height"]), ("png", WIDTH, HEIGHT))
        pixels = self.convert("-i", image, "-frames:v", "1", "-pix_fmt", "rgb24", "-f", "rawvideo", "-")
        self.assertEqual(len(pixels), WIDTH * HEIGHT * 3)
        # Probe well inside both uniform regions, away from compression edges.
        for x, y, expected in ((WIDTH // 4, HEIGHT // 4, 214), (3 * WIDTH // 4, 3 * HEIGHT // 4, 28)):
            offset = (y * WIDTH + x) * 3
            for channel in pixels[offset:offset + 3]:
                self.assertAlmostEqual(channel, expected, delta=12)

    def test_mp3_audio_conversion_and_decode(self):
        extracted = self.root / "extracted.mp3"
        self.convert("-i", self.media, "-map", "0:a:0", "-vn", "-c:a", "libmp3lame", "-b:a", "96k", extracted)
        metadata = self.probe(extracted, "-show_streams", "-show_format")
        self.assertEqual(len(metadata["streams"]), 1, metadata)
        audio = metadata["streams"][0]
        self.assertEqual(audio["codec_name"], "mp3", audio)
        self.assertEqual((int(audio["sample_rate"]), audio["channels"]), (SAMPLE_RATE, 1), audio)
        # MP3 container duration includes encoder delay and frame padding.
        self.assertAlmostEqual(float(metadata["format"]["duration"]), DURATION, delta=0.1, msg=metadata)
        pcm = self.convert(
            "-err_detect", "explode", "-i", extracted, "-map", "0:a:0", "-t", DURATION,
            "-c:a", "pcm_s16le", "-f", "s16le", "-",
        )
        self.assertEqual(len(pcm), len(self.samples) * 2)
        samples = struct.unpack(f"<{len(self.samples)}h", pcm)
        energy = sum(sample * sample for sample in samples)
        reference_energy = sum(sample * sample for sample in self.samples)
        self.assertGreater(energy, 0, "MP3 extraction is silent")
        correlation = sum(left * right for left, right in zip(samples, self.samples)) / math.sqrt(energy * reference_energy)
        self.assertGreater(correlation, 0.97, "Decoded MP3 does not match the synthetic 440 Hz input")

    def test_stream_copy_remux_preserves_every_packet(self):
        remuxed = self.root / "remuxed.mkv"
        self.convert("-i", self.media, "-map", "0:v:0", "-map", "0:a:0", "-c", "copy", remuxed)

        def packet_hashes(media):
            metadata = self.probe(
                media, "-show_packets", "-show_data_hash", "sha256",
                "-show_entries", "packet=stream_index,size,data_hash",
            )
            streams = {}
            for packet in metadata["packets"]:
                streams.setdefault(packet["stream_index"], []).append((packet["size"], packet["data_hash"]))
            return streams

        before, after = packet_hashes(self.media), packet_hashes(remuxed)
        self.assertEqual(set(before), {0, 1}, before)
        self.assertEqual(len(before[0]), FRAME_COUNT, before)
        self.assertTrue(before[1], "No AAC packets were encoded")
        self.assertEqual(after, before, "Remux changed encoded video/audio packet payloads")
        self.convert("-err_detect", "explode", "-i", remuxed, "-map", "0", "-f", "null", "-")

    def test_ffplay_version_without_opening_devices(self):
        if self.ffplay is None:
            self.skipTest("distribution has no sibling ffplay; set DOTFILES_TEST_FFPLAY if stored separately")
        check_version(self.ffplay, "ffplay")


if __name__ == "__main__":
    unittest.main()
