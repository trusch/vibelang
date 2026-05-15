#!/usr/bin/env python3
"""Run parse/runtime/audio smoke checks for examples/*/main.vibe racks."""

from __future__ import annotations

import argparse
import dataclasses
import json
import math
import os
import pathlib
import re
import signal
import struct
import subprocess
import sys
import tempfile
import time
import wave
from collections.abc import Iterable, Sequence


PROJECT_ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_VIBE_BIN = PROJECT_ROOT / "target/release/vibe"
PARSE_TEST = "rack_examples_parse"
PARSE_TEST_NAME = "rack_example_script_executes"
BAD_LOG_PATTERNS = re.compile(
    r"UGen ('.*' )?not installed|"
    r"failed to load synthdef|"
    r"SynthDef .*not found|"
    r"synthdef not found|"
    r"Message too long|"
    r"LocalBuf tried to allocate too many local buffers|"
    r"alloc failed|"
    r"Buffer UGen: no buffer data|"
    r"Too many grains",
    re.IGNORECASE,
)


@dataclasses.dataclass
class AudioMetrics:
    path: str
    frames: int
    channels: int
    sample_rate: int
    duration_seconds: float
    rms: float
    peak: float


@dataclasses.dataclass
class CheckResult:
    status: str
    detail: str = ""

    @property
    def ok(self) -> bool:
        return self.status in {"ok", "skip"}


@dataclasses.dataclass
class RackResult:
    rack: str
    example: str
    parse: CheckResult
    runtime: CheckResult
    audio: CheckResult
    baseline: AudioMetrics | None = None
    active: AudioMetrics | None = None
    rms_ratio: float | None = None
    peak_ratio: float | None = None
    log: str | None = None

    @property
    def ok(self) -> bool:
        return self.parse.ok and self.runtime.ok and self.audio.ok


def rack_examples(project_root: pathlib.Path) -> list[pathlib.Path]:
    return sorted(project_root.glob("examples/*/main.vibe"))


def run_command(
    args: Sequence[str],
    *,
    cwd: pathlib.Path,
    env: dict[str, str] | None = None,
    timeout: float | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        list(args),
        cwd=str(cwd),
        env=env,
        timeout=timeout,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def ensure_vibe_binary(vibe_bin: pathlib.Path, project_root: pathlib.Path, build: bool, force_build: bool) -> None:
    if not force_build and vibe_bin.is_file() and os.access(vibe_bin, os.X_OK):
        return
    if not build:
        raise RuntimeError(
            f"release binary not found or not executable: {vibe_bin}; "
            "run cargo build --release -p vibelang-cli"
        )

    result = run_command(
        ["cargo", "build", "--release", "-p", "vibelang-cli"],
        cwd=project_root,
        timeout=None,
    )
    if result.returncode != 0:
        raise RuntimeError("failed to build vibelang-cli:\n" + result.stdout[-4000:])


def parse_check(example: pathlib.Path, project_root: pathlib.Path) -> CheckResult:
    env = os.environ.copy()
    env["VIBE_RACK_EXAMPLE"] = str(example)
    env.setdefault("RUST_LOG", "warn")
    result = run_command(
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "vibelang-rhai",
            "--test",
            PARSE_TEST,
            PARSE_TEST_NAME,
            "--",
            "--exact",
            "--ignored",
            "--nocapture",
        ],
        cwd=project_root,
        env=env,
    )
    if result.returncode == 0:
        return CheckResult("ok")
    return CheckResult("fail", summarize_parse_failure(result.stdout))


def summarize_parse_failure(output: str) -> str:
    lines = output.splitlines()
    summary = []
    for index, line in enumerate(lines):
        if "failed to parse/execute:" in line:
            summary.append(line.strip())
            summary.extend(next_line.strip() for next_line in lines[index + 1 : index + 5] if next_line.strip())
            break
        if "has overflowed its stack" in line:
            summary.append(line.strip())
            break
        if "signal:" in line and "process didn't exit successfully" in line:
            summary.append(line.strip())
            break
    if not summary:
        summary = [line.strip() for line in lines[-12:] if line.strip()]
    return "\n".join(summary)[-2000:]


def runtime_command(vibe_bin: pathlib.Path, example: pathlib.Path, project_root: pathlib.Path) -> list[str]:
    return [
        str(vibe_bin),
        "run",
        "--no-watch",
        "--no-api",
        "--no-jack-connect",
        "-I",
        str(project_root),
        str(example),
    ]


def start_runtime(
    vibe_bin: pathlib.Path,
    example: pathlib.Path,
    project_root: pathlib.Path,
    log_file: pathlib.Path,
    data_home: pathlib.Path,
) -> subprocess.Popen[str]:
    env = os.environ.copy()
    env.setdefault("RUST_LOG", "info")
    env["XDG_DATA_HOME"] = str(data_home)
    log_handle = log_file.open("w", encoding="utf-8")
    return subprocess.Popen(
        runtime_command(vibe_bin, example, project_root),
        cwd=str(project_root),
        env=env,
        text=True,
        stdout=log_handle,
        stderr=subprocess.STDOUT,
    )


def stop_runtime(process: subprocess.Popen[str], grace_seconds: float = 5.0) -> int:
    if process.poll() is not None:
        return int(process.returncode)
    process.send_signal(signal.SIGINT)
    try:
        return int(process.wait(timeout=grace_seconds))
    except subprocess.TimeoutExpired:
        process.kill()
        return int(process.wait(timeout=grace_seconds))


def read_log(log_file: pathlib.Path) -> str:
    try:
        return log_file.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""


def evaluate_runtime_log(log_text: str, returncode: int, timed_out_by_runner: bool) -> CheckResult:
    bad_match = BAD_LOG_PATTERNS.search(log_text)
    if bad_match:
        return CheckResult("fail", f"known regression output: {bad_match.group(0)}")
    if "Transport started" not in log_text:
        return CheckResult("fail", "missing Transport started marker")
    if returncode not in (0, 130, -signal.SIGINT) and not timed_out_by_runner:
        return CheckResult("fail", f"process exited {returncode}")
    return CheckResult("ok")


def runtime_smoke(
    vibe_bin: pathlib.Path,
    example: pathlib.Path,
    project_root: pathlib.Path,
    seconds: float,
    work_dir: pathlib.Path,
) -> tuple[CheckResult, pathlib.Path]:
    log_file = work_dir / f"{example.parent.name}.runtime.log"
    data_home = work_dir / f"{example.parent.name}.xdg"
    data_home.mkdir()
    process = start_runtime(vibe_bin, example, project_root, log_file, data_home)
    timed_out = False
    try:
        process.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        timed_out = True
    returncode = stop_runtime(process)
    return evaluate_runtime_log(read_log(log_file), returncode, timed_out), log_file


def capture_wav(command_template: str, wav_path: pathlib.Path, seconds: float, rack: str, example: pathlib.Path) -> CheckResult:
    command = command_template.format(
        wav=str(wav_path),
        seconds=str(seconds),
        rack=rack,
        example=str(example),
    )
    try:
        result = subprocess.run(
            command,
            shell=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
            timeout=seconds + 10.0,
        )
    except subprocess.TimeoutExpired as exc:
        output = (exc.stdout or "") if isinstance(exc.stdout, str) else ""
        return CheckResult("fail", f"capture command timed out after {seconds + 10.0:g}s: {output[-1200:].strip()}")
    if result.returncode != 0:
        return CheckResult("fail", f"capture command exited {result.returncode}: {result.stdout[-1200:].strip()}")
    if not wav_path.is_file() or wav_path.stat().st_size == 0:
        return CheckResult("fail", f"capture command did not create WAV: {wav_path}")
    return CheckResult("ok")


def iter_pcm_samples(frames: bytes, sample_width: int) -> Iterable[float]:
    if sample_width == 1:
        for value in frames:
            yield (value - 128) / 128.0
    elif sample_width == 2:
        count = len(frames) // 2
        for (value,) in struct.iter_unpack("<h", frames[: count * 2]):
            yield value / 32768.0
    elif sample_width == 3:
        for index in range(0, len(frames) - 2, 3):
            raw = frames[index : index + 3]
            value = int.from_bytes(raw + (b"\xff" if raw[2] & 0x80 else b"\x00"), "little", signed=True)
            yield value / 8388608.0
    elif sample_width == 4:
        count = len(frames) // 4
        for (value,) in struct.iter_unpack("<i", frames[: count * 4]):
            yield value / 2147483648.0
    else:
        raise ValueError(f"unsupported WAV sample width: {sample_width}")


def analyze_wav(path: pathlib.Path) -> AudioMetrics:
    with wave.open(str(path), "rb") as wav:
        if wav.getcomptype() != "NONE":
            raise ValueError(f"unsupported compressed WAV type: {wav.getcomptype()}")
        channels = wav.getnchannels()
        sample_rate = wav.getframerate()
        frames_count = wav.getnframes()
        sample_width = wav.getsampwidth()
        raw = wav.readframes(frames_count)

    sample_count = 0
    square_sum = 0.0
    peak = 0.0
    for sample in iter_pcm_samples(raw, sample_width):
        sample_count += 1
        abs_sample = abs(sample)
        peak = max(peak, abs_sample)
        square_sum += sample * sample

    rms = math.sqrt(square_sum / sample_count) if sample_count else 0.0
    duration = frames_count / sample_rate if sample_rate else 0.0
    return AudioMetrics(
        path=str(path),
        frames=frames_count,
        channels=channels,
        sample_rate=sample_rate,
        duration_seconds=duration,
        rms=rms,
        peak=peak,
    )


def evaluate_audio(
    baseline: AudioMetrics,
    active: AudioMetrics,
    *,
    min_active_rms: float,
    min_active_peak: float,
    min_rms_ratio: float,
    min_peak_ratio: float,
    ratio_floor: float,
) -> tuple[CheckResult, float, float]:
    rms_ratio = active.rms / max(baseline.rms, ratio_floor)
    peak_ratio = active.peak / max(baseline.peak, ratio_floor)
    failures = []
    if active.rms < min_active_rms:
        failures.append(f"active_rms {active.rms:.6g} < {min_active_rms:.6g}")
    if active.peak < min_active_peak:
        failures.append(f"active_peak {active.peak:.6g} < {min_active_peak:.6g}")
    if rms_ratio < min_rms_ratio:
        failures.append(f"rms_ratio {rms_ratio:.3g} < {min_rms_ratio:.3g}")
    if peak_ratio < min_peak_ratio:
        failures.append(f"peak_ratio {peak_ratio:.3g} < {min_peak_ratio:.3g}")

    if failures:
        return CheckResult("fail", "; ".join(failures)), rms_ratio, peak_ratio
    return CheckResult("ok"), rms_ratio, peak_ratio


def runtime_with_audio(
    vibe_bin: pathlib.Path,
    example: pathlib.Path,
    project_root: pathlib.Path,
    args: argparse.Namespace,
    work_dir: pathlib.Path,
) -> tuple[CheckResult, CheckResult, AudioMetrics | None, AudioMetrics | None, float | None, float | None, pathlib.Path]:
    rack = example.parent.name
    log_file = work_dir / f"{rack}.runtime.log"
    data_home = work_dir / f"{rack}.xdg"
    data_home.mkdir()
    baseline_wav = work_dir / f"{rack}.baseline.wav"
    active_wav = work_dir / f"{rack}.active.wav"

    baseline_capture = capture_wav(args.capture_command, baseline_wav, args.baseline_seconds, rack, example)
    if not baseline_capture.ok:
        return CheckResult("skip"), baseline_capture, None, None, None, None, log_file

    process = start_runtime(vibe_bin, example, project_root, log_file, data_home)
    time.sleep(args.audio_warmup_seconds)
    active_capture = capture_wav(args.capture_command, active_wav, args.active_seconds, rack, example)
    ended_before_stop = process.poll() is not None
    returncode = stop_runtime(process)
    runtime = evaluate_runtime_log(
        read_log(log_file),
        returncode,
        timed_out_by_runner=not ended_before_stop,
    )
    if not active_capture.ok:
        return runtime, active_capture, None, None, None, None, log_file

    try:
        baseline = analyze_wav(baseline_wav)
        active = analyze_wav(active_wav)
        audio, rms_ratio, peak_ratio = evaluate_audio(
            baseline,
            active,
            min_active_rms=args.min_active_rms,
            min_active_peak=args.min_active_peak,
            min_rms_ratio=args.min_rms_ratio,
            min_peak_ratio=args.min_peak_ratio,
            ratio_floor=args.ratio_floor,
        )
    except Exception as exc:  # noqa: BLE001 - produce a concise audit failure
        return runtime, CheckResult("fail", str(exc)), None, None, None, None, log_file

    return runtime, audio, baseline, active, rms_ratio, peak_ratio, log_file


def run_rack(example: pathlib.Path, project_root: pathlib.Path, vibe_bin: pathlib.Path, args: argparse.Namespace, work_dir: pathlib.Path) -> RackResult:
    parse = CheckResult("skip", "--skip-parse")
    if not args.skip_parse:
        parse = parse_check(example, project_root)

    runtime = CheckResult("skip", "--skip-runtime")
    audio = CheckResult("skip", "no capture command; set --capture-command or VIBE_RACK_AUDIT_CAPTURE_CMD")
    baseline = None
    active = None
    rms_ratio = None
    peak_ratio = None
    log_file = None

    if not args.skip_runtime:
        if args.capture_command:
            runtime, audio, baseline, active, rms_ratio, peak_ratio, log_file = runtime_with_audio(
                vibe_bin, example, project_root, args, work_dir
            )
        else:
            runtime, log_file = runtime_smoke(
                vibe_bin,
                example,
                project_root,
                args.runtime_seconds,
                work_dir,
            )

    if args.require_audio and audio.status == "skip":
        audio = CheckResult("fail", audio.detail)

    return RackResult(
        rack=example.parent.name,
        example=str(example.relative_to(project_root)),
        parse=parse,
        runtime=runtime,
        audio=audio,
        baseline=baseline,
        active=active,
        rms_ratio=rms_ratio,
        peak_ratio=peak_ratio,
        log=str(log_file) if log_file else None,
    )


def fmt_float(value: float | None) -> str:
    if value is None:
        return "-"
    return f"{value:.6g}"


def print_summary(results: Sequence[RackResult], args: argparse.Namespace) -> None:
    print(
        "thresholds: "
        f"min_active_rms={args.min_active_rms:g} "
        f"min_active_peak={args.min_active_peak:g} "
        f"min_rms_ratio={args.min_rms_ratio:g} "
        f"min_peak_ratio={args.min_peak_ratio:g} "
        f"ratio_floor={args.ratio_floor:g}"
    )
    print(
        "rack                 parse runtime audio  "
        "base_rms  active_rms rms_ratio base_peak active_peak peak_ratio"
    )
    for result in results:
        print(
            f"{result.rack:<20} "
            f"{result.parse.status:<5} "
            f"{result.runtime.status:<7} "
            f"{result.audio.status:<6} "
            f"{fmt_float(result.baseline.rms if result.baseline else None):>8} "
            f"{fmt_float(result.active.rms if result.active else None):>10} "
            f"{fmt_float(result.rms_ratio):>9} "
            f"{fmt_float(result.baseline.peak if result.baseline else None):>9} "
            f"{fmt_float(result.active.peak if result.active else None):>11} "
            f"{fmt_float(result.peak_ratio):>10}"
        )
        for name, check in (("parse", result.parse), ("runtime", result.runtime), ("audio", result.audio)):
            if check.status == "fail":
                print(f"  {result.rack} {name} failure: {check.detail}")
        if result.log:
            print(f"  {result.rack} log: {result.log}")


def to_json(results: Sequence[RackResult]) -> str:
    return json.dumps([dataclasses.asdict(result) for result in results], indent=2, sort_keys=True)


def parser() -> argparse.ArgumentParser:
    arg_parser = argparse.ArgumentParser(
        description="Enumerate examples/*/main.vibe and report parse/runtime/audio smoke status.",
    )
    arg_parser.add_argument("--project-root", type=pathlib.Path, default=PROJECT_ROOT)
    arg_parser.add_argument("--vibe-bin", type=pathlib.Path, default=DEFAULT_VIBE_BIN)
    arg_parser.add_argument("--filter", help="substring filter for rack directory names")
    arg_parser.add_argument("--skip-build", action="store_true", help="do not build target/release/vibe if missing")
    arg_parser.add_argument("--force-build", action="store_true", help="rebuild target/release/vibe before runtime checks")
    arg_parser.add_argument("--skip-parse", action="store_true")
    arg_parser.add_argument("--skip-runtime", action="store_true")
    arg_parser.add_argument("--runtime-seconds", type=float, default=12.0)
    arg_parser.add_argument("--work-dir", type=pathlib.Path, help="keep logs/WAVs under this directory")
    arg_parser.add_argument("--json", action="store_true", help="print machine-readable JSON after the text summary")
    arg_parser.add_argument(
        "--capture-command",
        default=os.environ.get("VIBE_RACK_AUDIT_CAPTURE_CMD"),
        help=(
            "optional shell command that records a WAV; placeholders: "
            "{wav}, {seconds}, {rack}, {example}"
        ),
    )
    arg_parser.add_argument("--require-audio", action="store_true", help="fail if audio capture is skipped")
    arg_parser.add_argument("--baseline-seconds", type=float, default=3.0)
    arg_parser.add_argument("--active-seconds", type=float, default=6.0)
    arg_parser.add_argument("--audio-warmup-seconds", type=float, default=3.0)
    arg_parser.add_argument("--min-active-rms", type=float, default=0.0005)
    arg_parser.add_argument("--min-active-peak", type=float, default=0.002)
    arg_parser.add_argument("--min-rms-ratio", type=float, default=3.0)
    arg_parser.add_argument("--min-peak-ratio", type=float, default=1.5)
    arg_parser.add_argument("--ratio-floor", type=float, default=1e-9)
    return arg_parser


def main(argv: Sequence[str] | None = None) -> int:
    args = parser().parse_args(argv)
    project_root = args.project_root.resolve()
    vibe_bin = args.vibe_bin.resolve()
    examples = rack_examples(project_root)
    if args.filter:
        examples = [example for example in examples if args.filter in example.parent.name]
    if not examples:
        print("error: no examples/*/main.vibe rack examples found", file=sys.stderr)
        return 2

    if not args.skip_runtime:
        try:
            ensure_vibe_binary(
                vibe_bin,
                project_root,
                build=not args.skip_build,
                force_build=args.force_build,
            )
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2

    if args.work_dir:
        work_dir = args.work_dir.resolve()
        work_dir.mkdir(parents=True, exist_ok=True)
    else:
        default_parent = project_root / "target/rack-audit"
        default_parent.mkdir(parents=True, exist_ok=True)
        work_dir = pathlib.Path(tempfile.mkdtemp(prefix="run-", dir=default_parent))
    print(f"work_dir: {work_dir}")

    results = [run_rack(example, project_root, vibe_bin, args, work_dir) for example in examples]
    print_summary(results, args)
    if args.json:
        print(to_json(results))
    return 0 if all(result.ok for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
