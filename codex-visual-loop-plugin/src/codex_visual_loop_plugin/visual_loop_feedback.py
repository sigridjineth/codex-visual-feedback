#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from . import cli as rust_cli


def _parse_worker_identity(raw: Optional[str]) -> Tuple[Optional[str], Optional[str]]:
    if not raw or "/" not in raw:
        return None, None
    team, worker = raw.split("/", 1)
    team = team.strip()
    worker = worker.strip()
    if not team or not worker:
        return None, None
    return team, worker


def _read_json(path: Path) -> Dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}


def _resolve_team_state_root(
    team_name: Optional[str], worker_name: Optional[str], team_state_root_arg: Optional[str]
) -> Path:
    if team_state_root_arg:
        return Path(team_state_root_arg).expanduser().resolve()

    env_root = os.environ.get("OMX_TEAM_STATE_ROOT")
    if env_root:
        return Path(env_root).expanduser().resolve()

    fallback = (Path.cwd() / ".omx" / "state").resolve()

    if team_name and worker_name:
        identity_path = (
            fallback
            / "team"
            / team_name
            / "workers"
            / worker_name
            / "identity.json"
        )
        identity = _read_json(identity_path)
        identity_root = identity.get("team_state_root")
        if isinstance(identity_root, str) and identity_root.strip():
            return Path(identity_root).expanduser().resolve()

        config_path = fallback / "team" / team_name / "config.json"
        config = _read_json(config_path)
        config_root = config.get("team_state_root")
        if isinstance(config_root, str) and config_root.strip():
            return Path(config_root).expanduser().resolve()

        manifest_path = fallback / "team" / team_name / "manifest.v2.json"
        manifest = _read_json(manifest_path)
        manifest_root = manifest.get("team_state_root")
        if isinstance(manifest_root, str) and manifest_root.strip():
            return Path(manifest_root).expanduser().resolve()

    return fallback


def _resolve_inbox_path(
    inbox_arg: Optional[str],
    team_name: Optional[str],
    worker_name: Optional[str],
    team_state_root: Path,
) -> Path:
    if inbox_arg:
        return Path(inbox_arg).expanduser().resolve()

    if not team_name or not worker_name:
        raise ValueError(
            "Unable to resolve inbox path. Set OMX_TEAM_WORKER=<team>/<worker> or pass --inbox."
        )

    return (
        team_state_root
        / "team"
        / team_name
        / "workers"
        / worker_name
        / "inbox.md"
    ).resolve()


def _extract_first(text: str, patterns: List[str]) -> Optional[str]:
    for pattern in patterns:
        match = re.search(pattern, text, flags=re.IGNORECASE | re.MULTILINE)
        if match:
            value = (match.group(1) or "").strip()
            if value:
                return value
    return None


def _infer_process(text: str) -> Optional[str]:
    return _extract_first(
        text,
        [
            r"--process\s+\"([^\"]+)\"",
            r"--process\s+'([^']+)'",
            r"process\s+\"([^\"]+)\"",
            r"process\s+'([^']+)'",
            r"app\s+\"([^\"]+)\"",
            r"app\s+'([^']+)'",
        ],
    )


def _infer_action(text: str) -> Optional[str]:
    return _extract_first(
        text,
        [
            r"--action\s+\"([^\"]+)\"",
            r"--action\s+'([^']+)'",
            r"action\s+\"([^\"]+)\"",
            r"action\s+'([^']+)'",
        ],
    )


def _infer_action_cmd(text: str) -> Optional[str]:
    return _extract_first(
        text,
        [
            r"--action-cmd\s+\"([^\"]+)\"",
            r"--action-cmd\s+'([^']+)'",
        ],
    )


def _parse_actions(raw_actions: str) -> List[str]:
    items = [item.strip().lower() for item in raw_actions.split(",") if item.strip()]
    if not items:
        raise ValueError("No actions selected. Use --actions capture,ax-tree,observe.")
    allowed = {"capture", "ax-tree", "observe"}
    unknown = [item for item in items if item not in allowed]
    if unknown:
        raise ValueError(f"Unsupported action(s): {', '.join(unknown)}")
    return items


def _resolve_runner() -> List[str]:
    binary = rust_cli._resolve_rust_binary()  # noqa: SLF001 - same package, intentional.
    if binary is not None:
        return binary

    if shutil.which("cargo"):
        return [
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            str(rust_cli.MANIFEST_PATH),
            "--",
        ]

    codex_loop = shutil.which("codex-visual-loop")
    if codex_loop:
        return [codex_loop]

    raise RuntimeError(
        "Unable to locate codex-visual-loop runtime. Build Rust binary or install the plugin first."
    )


def _run_loop_command(runner: List[str], argv: List[str]) -> Dict[str, Any]:
    proc = subprocess.run(
        [*runner, *argv],
        check=False,
        capture_output=True,
        text=True,
    )

    stdout = proc.stdout.strip()
    stderr = proc.stderr.strip()
    parsed_stdout: Any = stdout
    if stdout:
        try:
            parsed_stdout = json.loads(stdout)
        except json.JSONDecodeError:
            parsed_stdout = stdout

    result: Dict[str, Any] = {
        "argv": argv,
        "returncode": proc.returncode,
        "stdout": parsed_stdout,
        "stderr": stderr,
    }

    if proc.returncode != 0:
        message = (
            f"Command failed ({proc.returncode}): {' '.join(argv)}"
            f"\nSTDERR: {stderr or '(empty)'}"
        )
        raise RuntimeError(message)

    return result


def _build_plan(
    inbox_text: str,
    process_override: Optional[str],
    action_override: Optional[str],
    action_cmd_override: Optional[str],
    allow_action_cmd: bool,
    actions: List[str],
    ax_depth: int,
    observe_duration: int,
) -> Dict[str, Any]:
    inferred_process = _infer_process(inbox_text)
    inferred_action = _infer_action(inbox_text)
    inferred_action_cmd = _infer_action_cmd(inbox_text)

    process_name = process_override or inferred_process
    action_label = action_override or inferred_action or "inbox-feedback"
    requested_action_cmd = action_cmd_override or inferred_action_cmd

    warnings: List[str] = []
    action_cmd = requested_action_cmd
    if action_cmd and not allow_action_cmd:
        warnings.append(
            "Ignored inferred/provided action command because --allow-action-cmd was not set."
        )
        action_cmd = None

    commands: List[Dict[str, Any]] = []

    for action in actions:
        if action == "capture":
            argv = ["capture", "--json"]
            if process_name:
                argv.extend(["--process", process_name])
            commands.append({"name": "capture", "argv": argv})
            continue

        if action == "ax-tree":
            argv = ["ax-tree", "--json", "--depth", str(ax_depth)]
            if process_name:
                argv.extend(["--process", process_name])
            commands.append({"name": "ax-tree", "argv": argv})
            continue

        if action == "observe":
            argv = [
                "observe",
                "--json",
                "--duration",
                str(max(observe_duration, 0)),
                "--action",
                action_label,
            ]
            if process_name:
                argv.extend(["--process", process_name])
            if action_cmd:
                argv.extend(["--action-cmd", action_cmd])
            commands.append({"name": "observe", "argv": argv})

    return {
        "process_name": process_name,
        "action_label": action_label,
        "inferred": {
            "process_name": inferred_process,
            "action_label": inferred_action,
            "action_cmd": inferred_action_cmd,
        },
        "warnings": warnings,
        "commands": commands,
    }


def run(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        prog="codex-visual-loop visual-loop-feedback",
        description=(
            "Read OMX team worker inbox instructions and prepare/run safe codex-visual-loop "
            "feedback actions (capture, ax-tree, observe)."
        ),
    )
    parser.add_argument("--inbox", help="Path to worker inbox.md. Auto-resolved from OMX_TEAM_WORKER if omitted.")
    parser.add_argument(
        "--team-state-root",
        help="Override OMX team state root. Defaults to OMX_TEAM_STATE_ROOT -> worker identity/config -> ./.omx/state",
    )
    parser.add_argument("--process", help="Force process name instead of inferring from inbox text.")
    parser.add_argument("--action", help="Force observe action label instead of inferring from inbox text.")
    parser.add_argument("--action-cmd", help="Optional action command for observe step (requires --allow-action-cmd).")
    parser.add_argument(
        "--allow-action-cmd",
        action="store_true",
        help="Allow executing action commands in observe step. Disabled by default for safety.",
    )
    parser.add_argument(
        "--actions",
        default="capture,ax-tree,observe",
        help="Comma-separated actions subset from: capture,ax-tree,observe",
    )
    parser.add_argument("--ax-depth", type=int, default=3, help="AX traversal depth (default: 3)")
    parser.add_argument(
        "--observe-duration",
        type=int,
        default=0,
        help="Observe duration seconds (default: 0 for fast safe mode)",
    )
    parser.add_argument("--execute", action="store_true", help="Execute planned codex-visual-loop commands.")
    parser.add_argument("--json", action="store_true", help="Emit JSON payload.")

    args = parser.parse_args(argv)

    worker_env = os.environ.get("OMX_TEAM_WORKER")
    team_name, worker_name = _parse_worker_identity(worker_env)

    try:
        team_state_root = _resolve_team_state_root(team_name, worker_name, args.team_state_root)
        inbox_path = _resolve_inbox_path(args.inbox, team_name, worker_name, team_state_root)
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    if not inbox_path.exists():
        print(f"error: inbox file not found: {inbox_path}", file=sys.stderr)
        return 2

    inbox_text = inbox_path.read_text(encoding="utf-8")

    try:
        actions = _parse_actions(args.actions)
        plan = _build_plan(
            inbox_text=inbox_text,
            process_override=args.process,
            action_override=args.action,
            action_cmd_override=args.action_cmd,
            allow_action_cmd=args.allow_action_cmd,
            actions=actions,
            ax_depth=args.ax_depth,
            observe_duration=args.observe_duration,
        )
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    payload: Dict[str, Any] = {
        "mode": "execute" if args.execute else "dry_run",
        "omx_team_worker": worker_env,
        "team_name": team_name,
        "worker_name": worker_name,
        "team_state_root": str(team_state_root),
        "inbox_path": str(inbox_path),
        "plan": plan,
    }

    if args.execute:
        try:
            runner = _resolve_runner()
            results: List[Dict[str, Any]] = []
            for command in plan["commands"]:
                results.append(_run_loop_command(runner, command["argv"]))
            payload["runner"] = runner
            payload["results"] = results
        except Exception as error:  # noqa: BLE001 - user-facing command wrapper
            payload["error"] = str(error)
            if args.json:
                print(json.dumps(payload, indent=2))
            else:
                print(f"[visual-loop-feedback] error: {error}", file=sys.stderr)
            return 1

    if args.json:
        print(json.dumps(payload, indent=2))
    else:
        print("[visual-loop-feedback] inbox:", payload["inbox_path"])
        for warning in plan["warnings"]:
            print(f"[visual-loop-feedback] warning: {warning}")
        for command in plan["commands"]:
            print("[visual-loop-feedback]", " ".join(["codex-visual-loop", *command["argv"]]))
        if args.execute:
            print("[visual-loop-feedback] executed", len(payload.get("results", [])), "command(s)")

    return 0


def main(argv: Optional[List[str]] = None) -> int:
    return run(argv)


if __name__ == "__main__":
    raise SystemExit(main())
