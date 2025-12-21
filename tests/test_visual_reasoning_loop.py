import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "codex-visual-loop-plugin" / "scripts"


def run_cmd(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        list(args),
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )


class VisualReasoningLoopTests(unittest.TestCase):
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

            run_cmd(
                "python3",
                str(SCRIPTS / "annotate_image.py"),
                str(input_path),
                str(output_path),
                "--spec",
                str(spec_path),
            )

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

            run_cmd(
                "python3",
                str(SCRIPTS / "compare_images.py"),
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

            run_cmd(
                "bash",
                str(SCRIPTS / "loop_compare.sh"),
                "--loop-dir",
                str(loop_dir),
                str(img1),
                "home",
            )

            run_cmd(
                "bash",
                str(SCRIPTS / "loop_compare.sh"),
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


if __name__ == "__main__":
    unittest.main()
