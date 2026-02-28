import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[1]
PLUGIN_ROOT = ROOT / "codex-visual-loop-plugin"
CLI = PLUGIN_ROOT / "src" / "codex_visual_loop_plugin" / "cli.py"
SKILL_PATH = ROOT / "skills" / "codex-visual-loop" / "SKILL.md"


def run_cli(
    *args: str,
    cwd: Path = ROOT,
    env_updates: dict[str, str] | None = None,
    unset_env: tuple[str, ...] = (),
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    for key in unset_env:
        env.pop(key, None)
    if env_updates:
        env.update(env_updates)

    return subprocess.run(
        [sys.executable, str(CLI), *args],
        cwd=cwd,
        check=check,
        capture_output=True,
        text=True,
        env=env,
    )


class CodexVisualLoopPluginTests(unittest.TestCase):
    def test_manifest_declares_required_features_and_commands(self):
        manifest = json.loads((PLUGIN_ROOT / "manifest.json").read_text(encoding="utf-8"))
        features = " ".join(manifest.get("features", []))
        self.assertIn("metadata", features.lower())
        self.assertIn("semantic", features.lower())
        self.assertIn("diff-to-bbox", features.lower())
        self.assertIn("observation packet", features.lower())
        self.assertIn("ax tree", features.lower())

        names = [item["name"] for item in manifest.get("commands", [])]
        for required in (
            "capture",
            "annotate",
            "diff",
            "loop",
            "observe",
            "ax-tree",
            "act",
            "explain-app",
            "visual-loop-feedback",
        ):
            self.assertIn(required, names)

    def test_cli_lists_commands(self):
        proc = run_cli("commands")
        payload = json.loads(proc.stdout)
        names = [item["name"] for item in payload["commands"]]
        self.assertIn("capture", names)
        self.assertIn("diff", names)
        self.assertIn("observe", names)
        self.assertIn("ax-tree", names)
        self.assertIn("act", names)
        self.assertIn("explain-app", names)
        for item in payload["commands"]:
            self.assertEqual(item.get("runner"), "rust")

    def test_skill_wrapper_mentions_required_commands(self):
        text = SKILL_PATH.read_text(encoding="utf-8").lower()
        self.assertIn("codex-visual-loop", text)
        for command in (
            "capture",
            "annotate",
            "diff",
            "loop",
            "observe",
            "ax-tree",
            "act",
            "explain-app",
            "visual-loop-feedback",
        ):
            self.assertIn(command, text)

    def test_capture_help_mentions_json_sidecar_flags(self):
        proc = run_cli("capture", "--help")
        stdout = proc.stdout.lower()
        self.assertIn("--json", stdout)
        self.assertIn("--sidecar", stdout)
        self.assertIn("--strict", stdout)
        self.assertIn("metadata", stdout)

    def test_capture_json_includes_capture_mode_and_fallback_fields(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            out = Path(tmpdir) / "capture.png"
            proc = run_cli("capture", str(out), "--json", "--no-sidecar")
            payload = json.loads(proc.stdout)
            self.assertIn(payload.get("capture_mode"), {"window", "screen", "fallback"})
            self.assertIsInstance(payload.get("fallback_used"), bool)
            self.assertIsInstance(payload.get("warnings"), list)
            probe = payload.get("window_probe")
            self.assertIsInstance(probe, dict)
            self.assertIn("selection_mode", probe)
            self.assertIn("candidate_count", probe)
            self.assertIn("usable", probe)

    def test_capture_strict_surfaces_explicit_failure_mode(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            out = Path(tmpdir) / "strict-capture.png"
            proc = run_cli(
                "capture",
                str(out),
                "--json",
                "--no-sidecar",
                "--strict",
                check=False,
                env_updates={"PATH": "/usr/bin"},
            )

            self.assertNotEqual(proc.returncode, 0)
            payload = json.loads(proc.stdout)
            self.assertTrue(payload.get("fallback_used"))
            self.assertEqual(payload.get("capture_mode"), "fallback")
            warning_text = " ".join(payload.get("warnings", [])).lower()
            self.assertIn("capture failed", warning_text)
            self.assertIn("placeholder output", proc.stderr.lower())

    def test_annotate_rel_units_and_semantic_fields(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            input_path = tmp / "input.png"
            output_path = tmp / "output.png"
            spec_path = tmp / "spec.json"

            Image.new("RGB", (200, 100), (255, 255, 255)).save(input_path)
            spec = {
                "defaults": {"units": "rel", "auto_fit": False},
                "annotations": [
                    {
                        "type": "rect",
                        "id": "cta",
                        "x": 0.1,
                        "y": 0.2,
                        "w": 0.5,
                        "h": 0.4,
                        "severity": "high",
                        "issue": "Button not centered",
                        "hypothesis": "Wrong spacing token",
                        "next_action": "Adjust margin",
                        "verify": "Center aligned",
                        "color": "#FF453A",
                    }
                ],
            }
            spec_path.write_text(json.dumps(spec), encoding="utf-8")

            run_cli("annotate", str(input_path), str(output_path), "--spec", str(spec_path))

            meta_path = output_path.with_suffix(".json")
            self.assertTrue(output_path.exists())
            self.assertTrue(meta_path.exists())

            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            rect = next(item for item in meta["annotations"] if item.get("id") == "cta")
            geom = rect["geometry"]
            self.assertAlmostEqual(geom["x"], 20, delta=1)
            self.assertAlmostEqual(geom["y"], 20, delta=1)
            self.assertAlmostEqual(geom["w"], 100, delta=1)
            self.assertAlmostEqual(geom["h"], 40, delta=1)
            self.assertEqual(rect["severity"], "high")
            self.assertEqual(rect["issue"], "Button not centered")
            self.assertEqual(rect["hypothesis"], "Wrong spacing token")
            self.assertEqual(rect["next_action"], "Adjust margin")
            self.assertEqual(rect["verify"], "Center aligned")

        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            input_path = tmp / "input.png"
            output_path = tmp / "output.png"
            spec_path = tmp / "spec.json"

            Image.new("RGB", (240, 120), (255, 255, 255)).save(input_path)
            spec = {
                "defaults": {
                    "units": "rel",
                    "auto_fit": True,
                    "anchor_pos": "center",
                    "anchor_offset": [0, -0.05],
                },
                "annotations": [
                    {"type": "rect", "id": "cta", "x": 0.25, "y": 0.3, "w": 0.3, "h": 0.25},
                    {"type": "spotlight", "id": "hero", "x": 0.55, "y": 0.15, "w": 0.35, "h": 0.55},
                    {"type": "arrow", "from": "cta", "to": "hero", "x1": 0.25, "y1": 0.45, "x2": 0.55, "y2": 0.2},
                    {
                        "type": "text",
                        "id": "cta-label",
                        "x": 0.25,
                        "y": 0.2,
                        "text": "Primary CTA",
                        "anchor": "cta",
                    },
                ],
            }
            spec_path.write_text(json.dumps(spec), encoding="utf-8")

            run_cli("annotate", str(input_path), str(output_path), "--spec", str(spec_path))

            meta = json.loads(output_path.with_suffix(".json").read_text(encoding="utf-8"))
            self.assertEqual(meta["defaults"]["auto_fit"], True)
            self.assertEqual(meta["defaults"]["anchor_pos"], "center")

            types = [item["type"] for item in meta["annotations"]]
            self.assertEqual(types, ["rect", "spotlight", "arrow", "text"])

            spotlight = next(item for item in meta["annotations"] if item["type"] == "spotlight")
            self.assertIn("bbox", spotlight["geometry_rel"])

            arrow = next(item for item in meta["annotations"] if item["type"] == "arrow")
            self.assertAlmostEqual(arrow["geometry"]["x1"], 96, delta=1)
            self.assertAlmostEqual(arrow["geometry"]["x2"], 174, delta=1)

            text = next(item for item in meta["annotations"] if item["type"] == "text")
            self.assertEqual(text.get("text"), "Primary CTA")

    def test_compare_images_emits_change_regions_and_spec(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            baseline = tmp / "baseline.png"
            current = tmp / "current.png"
            report = tmp / "report.json"
            spec = tmp / "change-spec.json"

            Image.new("RGB", (120, 80), (255, 255, 255)).save(baseline)
            image = Image.new("RGB", (120, 80), (255, 255, 255))
            draw = ImageDraw.Draw(image)
            draw.rectangle([20, 15, 55, 45], fill=(0, 0, 0))
            image.save(current)

            run_cli(
                "diff",
                str(baseline),
                str(current),
                "--json-out",
                str(report),
                "--annotate-spec-out",
                str(spec),
                "--bbox-threshold",
                "1",
                "--bbox-min-area",
                "10",
            )

            payload = json.loads(report.read_text(encoding="utf-8"))
            self.assertGreaterEqual(payload["change_region_count"], 1)
            first = payload["change_regions"][0]
            self.assertLessEqual(first["x"], 20)
            self.assertLessEqual(first["y"], 15)
            self.assertGreaterEqual(first["w"], 30)
            self.assertGreaterEqual(first["h"], 25)

            spec_payload = json.loads(spec.read_text(encoding="utf-8"))
            types = [item["type"] for item in spec_payload["annotations"]]
            self.assertIn("rect", types)
            self.assertIn("text", types)

    def test_loop_compare_generates_diff_annotated_artifact(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            img1 = tmp / "a.png"
            img2 = tmp / "b.png"
            loop_dir = tmp / "loop"

            Image.new("RGB", (100, 60), (255, 255, 255)).save(img1)
            img = Image.new("RGB", (100, 60), (255, 255, 255))
            draw = ImageDraw.Draw(img)
            draw.rectangle([10, 10, 40, 30], fill=(0, 0, 0))
            img.save(img2)

            run_cli("loop", "--loop-dir", str(loop_dir), str(img1), "home")
            run_cli(
                "loop",
                "--loop-dir",
                str(loop_dir),
                "--bbox-threshold",
                "1",
                "--bbox-min-area",
                "10",
                str(img2),
                "home",
            )

            annotations_dir = loop_dir / "annotations"
            reports_dir = loop_dir / "reports"
            self.assertTrue(any(annotations_dir.glob("home-*.png")))
            self.assertTrue(any(reports_dir.glob("home-*-change-spec.json")))

    def test_visual_loop_feedback_plans_actions_from_worker_inbox(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            state_root = tmp / ".omx" / "state"
            inbox = (
                state_root
                / "team"
                / "demo"
                / "workers"
                / "worker-2"
                / "inbox.md"
            )
            inbox.parent.mkdir(parents=True, exist_ok=True)
            inbox.write_text(
                """
Worker instruction: use codex-visual-loop capture --process "Finder" --json first.
Then validate with codex-visual-loop observe --action "open-settings" --action-cmd 'echo click'.
""".strip(),
                encoding="utf-8",
            )

            proc = run_cli(
                "visual-loop-feedback",
                "--json",
                env_updates={
                    "OMX_TEAM_WORKER": "demo/worker-2",
                    "OMX_TEAM_STATE_ROOT": str(state_root),
                },
            )

            payload = json.loads(proc.stdout)
            self.assertEqual(payload["mode"], "dry_run")
            self.assertEqual(payload["team_name"], "demo")
            self.assertEqual(payload["worker_name"], "worker-2")
            self.assertEqual(payload["inbox_path"], str(inbox.resolve()))

            command_names = [item["name"] for item in payload["plan"]["commands"]]
            self.assertEqual(command_names, ["capture", "ax-tree", "observe"])

            observe_argv = payload["plan"]["commands"][2]["argv"]
            self.assertIn("--action", observe_argv)
            self.assertNotIn("--action-cmd", observe_argv)
            self.assertTrue(payload["plan"]["warnings"])

    def test_visual_loop_feedback_supports_action_subset_for_explain_pipeline(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            state_root = tmp / ".omx" / "state"
            inbox = (
                state_root
                / "team"
                / "demo"
                / "workers"
                / "worker-7"
                / "inbox.md"
            )
            inbox.parent.mkdir(parents=True, exist_ok=True)
            inbox.write_text(
                """
Please inspect process "Calendar".
Use action "explain-toolbar-state".
""".strip(),
                encoding="utf-8",
            )

            proc = run_cli(
                "visual-loop-feedback",
                "--json",
                "--actions",
                "capture,observe",
                "--observe-duration",
                "0",
                env_updates={
                    "OMX_TEAM_WORKER": "demo/worker-7",
                    "OMX_TEAM_STATE_ROOT": str(state_root),
                },
            )

            payload = json.loads(proc.stdout)
            commands = payload["plan"]["commands"]
            self.assertEqual([cmd["name"] for cmd in commands], ["capture", "observe"])
            observe_argv = commands[1]["argv"]
            self.assertIn("--duration", observe_argv)
            self.assertIn("0", observe_argv)
            self.assertIn("--action", observe_argv)
            self.assertIn("explain-toolbar-state", observe_argv)
            self.assertEqual(payload["plan"]["inferred"]["process_name"], "Calendar")

    def test_visual_loop_feedback_allows_action_cmd_when_explicitly_enabled(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            state_root = tmp / ".omx" / "state"
            inbox = (
                state_root
                / "team"
                / "demo"
                / "workers"
                / "worker-2"
                / "inbox.md"
            )
            inbox.parent.mkdir(parents=True, exist_ok=True)
            inbox.write_text(
                'codex-visual-loop observe --process "Finder" --action "open" --action-cmd "echo click"',
                encoding="utf-8",
            )

            proc = run_cli(
                "visual-loop-feedback",
                "--json",
                "--allow-action-cmd",
                env_updates={
                    "OMX_TEAM_WORKER": "demo/worker-2",
                    "OMX_TEAM_STATE_ROOT": str(state_root),
                },
            )

            payload = json.loads(proc.stdout)
            observe_argv = payload["plan"]["commands"][2]["argv"]
            self.assertIn("--action-cmd", observe_argv)
            self.assertIn("echo click", observe_argv)
            self.assertFalse(payload["plan"]["warnings"])

    def test_visual_loop_feedback_help_mentions_safety_flags(self):
        proc = run_cli("visual-loop-feedback", "--help")
        stdout = proc.stdout.lower()
        self.assertIn("--execute", stdout)
        self.assertIn("--allow-action-cmd", stdout)
        self.assertIn("--team-state-root", stdout)
        self.assertIn("--actions", stdout)
        self.assertIn("--observe-duration", stdout)

    def test_observe_and_ax_tree_help_surface_expected_flags(self):
        observe = run_cli("observe", "--help")
        observe_stdout = observe.stdout.lower()
        self.assertIn("--action", observe_stdout)
        self.assertIn("--action-cmd", observe_stdout)
        self.assertIn("observation packet", observe_stdout)

        ax_tree = run_cli("ax-tree", "--help")
        ax_stdout = ax_tree.stdout.lower()
        self.assertIn("--depth", ax_stdout)
        self.assertIn("--json", ax_stdout)
        self.assertIn("accessibility", ax_stdout)

    def test_act_help_and_dry_run_payload(self):
        help_proc = run_cli("act", "--help")
        help_stdout = help_proc.stdout.lower()
        self.assertIn("--click", help_stdout)
        self.assertIn("--click-rel", help_stdout)
        self.assertIn("--text", help_stdout)
        self.assertIn("--hotkey", help_stdout)
        self.assertIn("--dry-run", help_stdout)

        dry_proc = run_cli(
            "act",
            "--process",
            "DemoApp",
            "--click",
            "120,80",
            "--text",
            "hello",
            "--hotkey",
            "cmd+l",
            "--enter",
            "--tab",
            "2",
            "--dry-run",
            "--json",
        )
        payload = json.loads(dry_proc.stdout)
        self.assertTrue(payload["dry_run"])
        self.assertEqual(payload["process_name"], "DemoApp")
        action_types = [item["type"] for item in payload["actions"]]
        self.assertIn("click", action_types)
        self.assertIn("type", action_types)
        self.assertIn("hotkey", action_types)
        self.assertIn("tab", action_types)
        self.assertIn("enter", action_types)

    def test_observe_json_contains_packet_fields_for_explain_pipeline(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            out_dir = tmp / "observe"
            proc = run_cli(
                "observe",
                "--process",
                "DemoApp",
                "--action",
                "explain-app-state",
                "--duration",
                "0",
                "--out-dir",
                str(out_dir),
                "--json",
            )
            payload = json.loads(proc.stdout)

            self.assertEqual(payload["action"]["label"], "explain-app-state")
            self.assertIn("before_capture", payload)
            self.assertIn("after_capture", payload)
            self.assertIn("clip", payload)
            self.assertIn("diff", payload)
            self.assertIn("change_regions", payload["diff"])
            self.assertTrue(Path(payload["before_capture"]["capture_path"]).exists())
            self.assertTrue(Path(payload["after_capture"]["capture_path"]).exists())
            self.assertTrue(Path(payload["action"]["log_path"]).exists())
            self.assertTrue(Path(payload["clip"]["video_path"]).exists())

    def test_observe_duration_three_waits_multiple_seconds(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            out_dir = tmp / "observe-duration"
            start = time.monotonic()
            proc = run_cli(
                "observe",
                "--process",
                "DemoApp",
                "--action",
                "timing-check",
                "--duration",
                "3",
                "--out-dir",
                str(out_dir),
                "--json",
            )
            elapsed = time.monotonic() - start
            payload = json.loads(proc.stdout)
            self.assertEqual(payload["clip"]["duration_sec"], 3)
            self.assertGreaterEqual(elapsed, 2.5)

    def test_explain_app_generates_packet_prompt_report_without_codex(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            out_dir = tmp / "explain"
            proc = run_cli(
                "explain-app",
                "--process",
                "DemoApp",
                "--no-codex",
                "--out-dir",
                str(out_dir),
                "--json",
            )
            payload = json.loads(proc.stdout)
            self.assertEqual(payload["mode"], "fallback")
            self.assertTrue(Path(payload["packet_path"]).exists())
            self.assertTrue(Path(payload["prompt_path"]).exists())
            self.assertTrue(Path(payload["report_path"]).exists())


if __name__ == "__main__":
    unittest.main()
