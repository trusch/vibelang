import importlib.util
import math
import pathlib
import struct
import sys
import tempfile
import unittest
import wave


SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "scripts/rack_smoke_audio_audit.py"
SPEC = importlib.util.spec_from_file_location("rack_smoke_audio_audit", SCRIPT)
audit = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = audit
SPEC.loader.exec_module(audit)


def write_wav(path: pathlib.Path, samples: list[float], sample_rate: int = 48_000) -> None:
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        frames = b"".join(
            struct.pack("<h", max(-32768, min(32767, int(sample * 32767))))
            for sample in samples
        )
        wav.writeframes(frames)


class RackSmokeAudioAuditTests(unittest.TestCase):
    def test_analyze_wav_reports_rms_and_peak_without_audioop(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = pathlib.Path(temp) / "tone.wav"
            samples = [0.0, 0.5, -0.5, 1.0, -1.0]
            write_wav(path, samples)

            metrics = audit.analyze_wav(path)

            self.assertEqual(metrics.frames, len(samples))
            self.assertEqual(metrics.channels, 1)
            self.assertAlmostEqual(metrics.peak, 0.999969, places=5)
            self.assertAlmostEqual(metrics.rms, math.sqrt(0.5), places=4)

    def test_audio_thresholds_reject_silence(self) -> None:
        baseline = audit.AudioMetrics("baseline.wav", 10, 1, 48_000, 0.1, 0.0, 0.0)
        active = audit.AudioMetrics("active.wav", 10, 1, 48_000, 0.1, 0.0, 0.0)

        result, rms_ratio, peak_ratio = audit.evaluate_audio(
            baseline,
            active,
            min_active_rms=0.0005,
            min_active_peak=0.002,
            min_rms_ratio=3.0,
            min_peak_ratio=1.5,
            ratio_floor=1e-9,
        )

        self.assertEqual(result.status, "fail")
        self.assertIn("active_rms", result.detail)
        self.assertEqual(rms_ratio, 0.0)
        self.assertEqual(peak_ratio, 0.0)

    def test_audio_thresholds_accept_active_signal_above_baseline(self) -> None:
        baseline = audit.AudioMetrics("baseline.wav", 10, 1, 48_000, 0.1, 0.001, 0.002)
        active = audit.AudioMetrics("active.wav", 10, 1, 48_000, 0.1, 0.01, 0.02)

        result, rms_ratio, peak_ratio = audit.evaluate_audio(
            baseline,
            active,
            min_active_rms=0.0005,
            min_active_peak=0.002,
            min_rms_ratio=3.0,
            min_peak_ratio=1.5,
            ratio_floor=1e-9,
        )

        self.assertEqual(result.status, "ok")
        self.assertAlmostEqual(rms_ratio, 10.0)
        self.assertAlmostEqual(peak_ratio, 10.0)

    def test_parse_failure_summary_removes_cargo_warning_noise(self) -> None:
        output = """
warning: multiple fields are never read
running 1 test
thread 'rack_example_script_executes' panicked at tests/rack_examples_parse.rs:
/repo/examples/rack/main.vibe failed to parse/execute: Script error: Module not found: stdlib/effects/reverb.vibe (line 15, position 8)
note: run with `RUST_BACKTRACE=1`
test result: FAILED
"""

        summary = audit.summarize_parse_failure(output)

        self.assertIn("failed to parse/execute", summary)
        self.assertIn("Module not found", summary)
        self.assertNotIn("multiple fields", summary)

    def test_runtime_rust_log_keeps_transport_marker_visible_under_scoped_filter(self) -> None:
        self.assertEqual(audit.rust_log_with_info("ewa=info"), "ewa=info,info")

    def test_runtime_rust_log_preserves_broad_debug_filter(self) -> None:
        self.assertEqual(audit.rust_log_with_info("debug"), "debug")

    def test_prepare_xdg_data_home_links_user_supercollider_extensions(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            data_home = root / "xdg"
            user_extensions = root / "user-extensions"
            data_home.mkdir()
            user_extensions.mkdir()

            audit.prepare_xdg_data_home(data_home, user_extensions)

            link = data_home / "SuperCollider" / "Extensions"
            self.assertTrue(link.is_symlink())
            self.assertEqual(link.resolve(), user_extensions)

    def test_prepare_xdg_data_home_ignores_missing_user_extensions(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            data_home = pathlib.Path(temp) / "xdg"
            data_home.mkdir()

            audit.prepare_xdg_data_home(data_home, pathlib.Path(temp) / "missing")

            self.assertFalse((data_home / "SuperCollider" / "Extensions").exists())


if __name__ == "__main__":
    unittest.main()
