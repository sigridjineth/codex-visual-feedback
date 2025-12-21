#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOT'
capture_app_window.sh

Usage:
  capture_app_window.sh [options] [out_path] [process_name]

Options:
  --out <path>       Output PNG path (same as first positional arg)
  --process <name>   Target app process (same as second positional arg)
  --step <name>      Optional workflow step label (e.g. before/after)
  --note <text>      Optional free-form note stored in metadata
  --json             Print capture metadata JSON to stdout
  --sidecar <path>   Custom metadata sidecar path (default: <out>.json)
  --no-sidecar       Disable sidecar metadata file generation
  -h, --help         Show help

Defaults:
  out_path     .codex-visual-loop/capture/app-window-<app>-YYYYMMDD-HHMMSS-<pid>-<rand>.png
  process_name frontmost app

Env:
  CVLP_OUT_DIR override default output root (default: .codex-visual-loop)

Output:
  - By default, prints the output image path.
  - Also writes a JSON sidecar next to the image unless --no-sidecar is provided.
EOT
}

out=""
process=""
step=""
note=""
print_json=0
write_sidecar=1
sidecar=""

positionals=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      out="${2:-}"
      shift 2
      ;;
    --process)
      process="${2:-}"
      shift 2
      ;;
    --step)
      step="${2:-}"
      shift 2
      ;;
    --note)
      note="${2:-}"
      shift 2
      ;;
    --json)
      print_json=1
      shift
      ;;
    --sidecar)
      sidecar="${2:-}"
      shift 2
      ;;
    --no-sidecar)
      write_sidecar=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      while [[ $# -gt 0 ]]; do
        positionals+=("$1")
        shift
      done
      ;;
    -*)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      positionals+=("$1")
      shift
      ;;
  esac
done

if [[ -z "${out}" && ${#positionals[@]} -ge 1 ]]; then
  out="${positionals[0]}"
fi
if [[ -z "${process}" && ${#positionals[@]} -ge 2 ]]; then
  process="${positionals[1]}"
fi
if [[ ${#positionals[@]} -gt 2 ]]; then
  echo "error: too many positional arguments" >&2
  usage >&2
  exit 1
fi

captures_dir="${out_root}/capture"
ts=$(date +%Y%m%d-%H%M%S)
ts_iso=$(date -u +%Y-%m-%dT%H:%M:%SZ)
epoch_ms=$(( $(date +%s) * 1000 ))

if [[ -z "${process}" ]]; then
  process=$(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' 2>/dev/null || true)
fi

slug=$(echo "${process}" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9._-')
if [[ -z "${slug}" ]]; then
  slug="app"
fi

if [[ -z "${out}" ]]; then
  out="${captures_dir}/app-window-${slug}-${ts}-$$-$RANDOM.png"
fi

pos=$(osascript -e "tell application \"System Events\" to tell process \"${process}\" to get position of window 1" 2>/dev/null || true)
size=$(osascript -e "tell application \"System Events\" to tell process \"${process}\" to get size of window 1" 2>/dev/null || true)
window_title=$(osascript -e "tell application \"System Events\" to tell process \"${process}\" to get name of window 1" 2>/dev/null || true)

if [[ -z "${pos}" || -z "${size}" ]]; then
  echo "error: window not found for process '${process}'" >&2
  echo "hint: verify app is running, Accessibility enabled for terminal, and process name (try exact app name)" >&2
  exit 1
fi

pos=$(echo "${pos}" | tr -d ' ')
size=$(echo "${size}" | tr -d ' ')

x=${pos%,*}
y=${pos#*,}
w=${size%,*}
h=${size#*,}

mkdir -p "$(dirname "${out}")"
screencapture -x -R "${x},${y},${w},${h}" "${out}"
image_w=$(sips -g pixelWidth "${out}" 2>/dev/null | awk '/pixelWidth:/ {print $2; exit}')
image_h=$(sips -g pixelHeight "${out}" 2>/dev/null | awk '/pixelHeight:/ {print $2; exit}')
if [[ -z "${image_w}" ]]; then image_w="${w}"; fi
if [[ -z "${image_h}" ]]; then image_h="${h}"; fi

sidecar_out=""
if [[ ${write_sidecar} -eq 1 ]]; then
  if [[ -z "${sidecar}" ]]; then
    sidecar_out="${out%.*}.json"
  else
    sidecar_out="${sidecar}"
  fi
  mkdir -p "$(dirname "${sidecar_out}")"
fi

payload=$(CAPTURE_PATH="${out}" SIDECAR_PATH="${sidecar_out}" PROCESS_NAME="${process}" APP_SLUG="${slug}" WINDOW_TITLE="${window_title}" TS_ISO="${ts_iso}" TS_EPOCH_MS="${epoch_ms}" STEP_LABEL="${step}" NOTE_TEXT="${note}" X="${x}" Y="${y}" W="${w}" H="${h}" IMAGE_W="${image_w}" IMAGE_H="${image_h}" WRITE_SIDECAR="${write_sidecar}" \
python3 - <<'PY'
import json
import os


def _as_int(name: str) -> int:
    try:
        return int(float(os.environ.get(name, "0")))
    except Exception:
        return 0


capture_path = os.path.abspath(os.environ.get("CAPTURE_PATH") or "")
sidecar_path = os.environ.get("SIDECAR_PATH") or ""
if sidecar_path:
    sidecar_path = os.path.abspath(sidecar_path)

x = _as_int("X")
y = _as_int("Y")
w = _as_int("W")
h = _as_int("H")
image_w = _as_int("IMAGE_W")
image_h = _as_int("IMAGE_H")
scale_x = (image_w / w) if w else None
scale_y = (image_h / h) if h else None
scale = None
if scale_x is not None and scale_y is not None:
    scale = round((scale_x + scale_y) / 2, 6)

payload = {
    "image_path": capture_path,
    "capture_path": capture_path,
    "sidecar_path": sidecar_path or None,
    "captured_at": os.environ.get("TS_ISO") or None,
    "captured_at_epoch_ms": _as_int("TS_EPOCH_MS"),
    "app_name": os.environ.get("PROCESS_NAME") or None,
    "app_slug": os.environ.get("APP_SLUG") or None,
    "window_title": os.environ.get("WINDOW_TITLE") or None,
    "step": os.environ.get("STEP_LABEL") or None,
    "note": os.environ.get("NOTE_TEXT") or None,
    "bounds": {
        "x": x,
        "y": y,
        "w": w,
        "h": h,
        "units": "pt",
    },
    "image_size": {
        "w": image_w,
        "h": image_h,
        "units": "px",
    },
    "scale": round(scale, 6) if isinstance(scale, float) else None,
    "scale_x": round(scale_x, 6) if isinstance(scale_x, float) else None,
    "scale_y": round(scale_y, 6) if isinstance(scale_y, float) else None,
    "window": {
        "x": x,
        "y": y,
        "w": w,
        "h": h,
        "x2": x + w,
        "y2": y + h,
        "units": "px",
    },
    "capture_tool": "capture_app_window.sh",
    "capture_sidecar_version": 1,
}

if os.environ.get("WRITE_SIDECAR") == "1" and sidecar_path:
    with open(sidecar_path, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2)

print(json.dumps(payload))
PY
)

if [[ ${print_json} -eq 1 ]]; then
  echo "${payload}"
else
  echo "${out}"
fi
