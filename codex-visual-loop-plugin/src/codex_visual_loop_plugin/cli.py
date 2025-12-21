#!/usr/bin/env python3
from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path


PLUGIN_ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = PLUGIN_ROOT / "Cargo.toml"
ENV_BIN = "CODEX_VISUAL_LOOP_RUST_BIN"


def _candidate_binaries() -> list[Path]:
    return [
        PLUGIN_ROOT / "target" / "debug" / "codex-visual-loop",
        PLUGIN_ROOT / "target" / "release" / "codex-visual-loop",
    ]


def _is_executable(path: Path) -> bool:
    return path.exists() and os.access(path, os.X_OK)


def _binary_is_fresh(path: Path) -> bool:
    try:
        binary_mtime = path.stat().st_mtime
    except OSError:
        return False

    source_paths = [
        PLUGIN_ROOT / "src-rs" / "main.rs",
        PLUGIN_ROOT / "Cargo.toml",
    ]
    for source in source_paths:
        try:
            if source.exists() and source.stat().st_mtime > binary_mtime:
                return False
        except OSError:
            return False
    return True


def _resolve_rust_binary() -> list[str] | None:
    env_path = os.environ.get(ENV_BIN)
    if env_path:
        env_bin = Path(env_path).expanduser()
        if _is_executable(env_bin) and _binary_is_fresh(env_bin):
            return [str(env_bin)]

    for candidate in _candidate_binaries():
        if _is_executable(candidate) and _binary_is_fresh(candidate):
            return [str(candidate)]

    return None


def _run_rust(argv: list[str]) -> int:
    binary = _resolve_rust_binary()
    if binary is not None:
        cmd = [*binary, *argv]
    else:
        if not shutil.which("cargo"):
            print(
                "error: Rust binary not found and cargo is unavailable. "
                f"Set {ENV_BIN} or build with `cargo build --manifest-path {MANIFEST_PATH}`.",
                file=sys.stderr,
            )
            return 2
        cmd = [
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            str(MANIFEST_PATH),
            "--",
            *argv,
        ]

    proc = subprocess.run(cmd)
    return int(proc.returncode)


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)

    if args and args[0] == "visual-loop-feedback":
        try:
            from .visual_loop_feedback import main as feedback_main
        except ImportError:
            sys.path.insert(0, str(PLUGIN_ROOT / "src"))
            from codex_visual_loop_plugin.visual_loop_feedback import main as feedback_main

        return int(feedback_main(args[1:]))

    return _run_rust(args)


if __name__ == "__main__":
    raise SystemExit(main())
