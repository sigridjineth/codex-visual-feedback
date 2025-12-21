#!/usr/bin/env python3
import argparse
import json
import os
import sys
from typing import Dict, List

try:
    from PIL import Image, ImageChops, ImageDraw, ImageFont
except Exception:
    print("error: Pillow is required. Install with: python3 -m pip install pillow", file=sys.stderr)
    sys.exit(2)


def load_image(path: str) -> Image.Image:
    return Image.open(path).convert("RGBA")


def _ensure_parent(path: str) -> None:
    parent = os.path.dirname(path)
    if parent:
        os.makedirs(parent, exist_ok=True)


def _extract_change_regions(
    gray: Image.Image,
    threshold: int,
    min_pixels: int,
    pad: int,
    max_boxes: int,
) -> List[Dict]:
    width, height = gray.size
    pix = gray.load()
    active = bytearray(width * height)
    visited = bytearray(width * height)

    for y in range(height):
        row = y * width
        for x in range(width):
            if pix[x, y] > threshold:
                active[row + x] = 1

    regions: List[Dict] = []

    for y in range(height):
        for x in range(width):
            start = y * width + x
            if visited[start] or not active[start]:
                continue

            stack = [start]
            visited[start] = 1
            minx = maxx = x
            miny = maxy = y
            changed_pixels = 0

            while stack:
                node = stack.pop()
                cx = node % width
                cy = node // width
                changed_pixels += 1

                if cx < minx:
                    minx = cx
                if cx > maxx:
                    maxx = cx
                if cy < miny:
                    miny = cy
                if cy > maxy:
                    maxy = cy

                if cx > 0:
                    left = node - 1
                    if active[left] and not visited[left]:
                        visited[left] = 1
                        stack.append(left)
                if cx < width - 1:
                    right = node + 1
                    if active[right] and not visited[right]:
                        visited[right] = 1
                        stack.append(right)
                if cy > 0:
                    up = node - width
                    if active[up] and not visited[up]:
                        visited[up] = 1
                        stack.append(up)
                if cy < height - 1:
                    down = node + width
                    if active[down] and not visited[down]:
                        visited[down] = 1
                        stack.append(down)

            if changed_pixels < max(1, min_pixels):
                continue

            x0 = max(0, minx - pad)
            y0 = max(0, miny - pad)
            x1 = min(width - 1, maxx + pad)
            y1 = min(height - 1, maxy + pad)
            box_w = (x1 - x0) + 1
            box_h = (y1 - y0) + 1
            box_area = box_w * box_h

            regions.append(
                {
                    "x": x0,
                    "y": y0,
                    "w": box_w,
                    "h": box_h,
                    "x2": x0 + box_w,
                    "y2": y0 + box_h,
                    "pixels": int(changed_pixels),
                    "area": int(box_area),
                    "coverage": round((changed_pixels / box_area), 4) if box_area else 0.0,
                    "intent": "changed-region",
                    "action": "inspect",
                }
            )

    regions.sort(key=lambda item: item["pixels"], reverse=True)
    if max_boxes > 0:
        regions = regions[:max_boxes]

    for idx, region in enumerate(regions, start=1):
        region["id"] = f"change-{idx}"
        region["rel"] = {
            "x": round(region["x"] / width, 6) if width else 0.0,
            "y": round(region["y"] / height, 6) if height else 0.0,
            "w": round(region["w"] / width, 6) if width else 0.0,
            "h": round(region["h"] / height, 6) if height else 0.0,
        }

    return regions


def _build_annotation_spec(regions: List[Dict]) -> Dict:
    annotations = []
    for idx, region in enumerate(regions, start=1):
        rect_id = region.get("id") or f"change-{idx}"
        annotations.append(
            {
                "type": "rect",
                "id": rect_id,
                "x": region["x"],
                "y": region["y"],
                "w": region["w"],
                "h": region["h"],
                "color": "#FF453A",
                "width": 3,
                "intent": "changed-region",
                "action": "inspect",
            }
        )
        annotations.append(
            {
                "type": "text",
                "text": f"Δ{idx}",
                "anchor": rect_id,
                "anchor_pos": "top_left",
                "anchor_offset": [0, -18],
                "color": "#FFFFFF",
                "text_bg": "rgba(255,69,58,0.78)",
                "intent": "change-label",
                "action": "review-diff",
            }
        )

    return {
        "defaults": {
            "auto_scale": True,
            "outline": True,
            "text_bg": "rgba(0,0,0,0.6)",
        },
        "annotations": annotations,
    }


def _draw_annotations(current: Image.Image, gray: Image.Image, regions: List[Dict], out_path: str) -> str:
    _ensure_parent(out_path)
    overlay = Image.new("RGBA", current.size, (255, 0, 0, 0))
    overlay.putalpha(gray)
    vis = Image.alpha_composite(current, overlay)

    draw = ImageDraw.Draw(vis)
    try:
        font = ImageFont.load_default()
    except Exception:
        font = None

    for idx, region in enumerate(regions, start=1):
        x = int(region["x"])
        y = int(region["y"])
        w = int(region["w"])
        h = int(region["h"])
        draw.rectangle([x, y, x + w, y + h], outline=(255, 69, 58, 255), width=3)
        if font:
            label = f"Δ{idx}"
            tx = x + 4
            ty = max(0, y - 16)
            try:
                bbox = draw.textbbox((tx, ty), label, font=font)
            except Exception:
                label = f"D{idx}"
                bbox = draw.textbbox((tx, ty), label, font=font)
            draw.rectangle([bbox[0] - 2, bbox[1] - 1, bbox[2] + 2, bbox[3] + 1], fill=(255, 69, 58, 220))
            try:
                draw.text((tx, ty), label, fill=(255, 255, 255, 255), font=font)
            except Exception:
                draw.text((tx, ty), f"{idx}", fill=(255, 255, 255, 255), font=font)

    vis.convert("RGB").save(out_path)
    return os.path.abspath(out_path)


def main() -> int:
    parser = argparse.ArgumentParser(description="Compare two images and output diff metrics.")
    parser.add_argument("baseline", help="Path to baseline image")
    parser.add_argument("current", help="Path to current image")
    parser.add_argument("--diff-out", help="Path to write diff image (PNG)")
    parser.add_argument("--json-out", help="Path to write JSON report")
    parser.add_argument("--resize", action="store_true", help="Resize current to baseline size")
    parser.add_argument("--bbox-threshold", type=int, default=24, help="Pixel diff threshold (default: 24)")
    parser.add_argument("--bbox-min-area", type=int, default=64, help="Minimum changed pixels per region (default: 64)")
    parser.add_argument("--bbox-pad", type=int, default=2, help="Padding around each bbox (default: 2)")
    parser.add_argument("--max-boxes", type=int, default=16, help="Maximum number of change regions (default: 16)")
    parser.add_argument("--annotated-out", help="Path to write annotated current image with change boxes")
    parser.add_argument("--annotate-spec-out", help="Path to write annotate_image-compatible JSON spec")
    args = parser.parse_args()

    if not os.path.exists(args.baseline):
        print(f"error: baseline not found: {args.baseline}", file=sys.stderr)
        return 1
    if not os.path.exists(args.current):
        print(f"error: current not found: {args.current}", file=sys.stderr)
        return 1

    baseline = load_image(args.baseline)
    current = load_image(args.current)
    resized = False

    if baseline.size != current.size:
        if args.resize:
            current = current.resize(baseline.size, Image.LANCZOS)
            resized = True
        else:
            print("error: image sizes differ. Re-run with --resize to match baseline size.", file=sys.stderr)
            return 1

    diff = ImageChops.difference(baseline, current)
    gray = diff.convert("L")
    hist = gray.histogram()
    total = sum(hist)
    changed = total - hist[0] if total else 0
    avg = sum(i * c for i, c in enumerate(hist)) / (255 * total) if total else 0.0
    percent_changed = (changed / total * 100) if total else 0.0
    avg_diff_percent = avg * 100

    regions = _extract_change_regions(
        gray,
        threshold=max(0, min(255, int(args.bbox_threshold))),
        min_pixels=max(1, int(args.bbox_min_area)),
        pad=max(0, int(args.bbox_pad)),
        max_boxes=max(0, int(args.max_boxes)),
    )

    diff_path = None
    if args.diff_out:
        diff_path = args.diff_out
        _ensure_parent(diff_path)
        overlay = Image.new("RGBA", current.size, (255, 0, 0, 0))
        overlay.putalpha(gray)
        Image.alpha_composite(current, overlay).convert("RGB").save(diff_path)

    annotate_spec_path = None
    annotate_spec = _build_annotation_spec(regions)
    if args.annotate_spec_out:
        annotate_spec_path = os.path.abspath(args.annotate_spec_out)
        _ensure_parent(annotate_spec_path)
        with open(annotate_spec_path, "w", encoding="utf-8") as f:
            json.dump(annotate_spec, f, indent=2)

    annotated_path = None
    if args.annotated_out:
        annotated_path = _draw_annotations(current, gray, regions, args.annotated_out)

    result = {
        "baseline": os.path.abspath(args.baseline),
        "current": os.path.abspath(args.current),
        "diff_image": os.path.abspath(diff_path) if diff_path else None,
        "annotated_image": annotated_path,
        "annotate_spec": annotate_spec_path,
        "percent_changed": round(percent_changed, 3),
        "avg_diff_percent": round(avg_diff_percent, 3),
        "size": {"width": baseline.size[0], "height": baseline.size[1]},
        "resized": resized,
        "change_regions": regions,
        "change_region_count": len(regions),
    }

    if args.json_out:
        _ensure_parent(args.json_out)
        with open(args.json_out, "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2)

    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
