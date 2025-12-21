#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
loop_compare.sh

Usage:
  loop_compare.sh [options] <current_path> <baseline_name>

Options:
  --loop-dir <path>   Override loop storage directory (default: $CVLP_LOOP_DIR or .codex-visual-loop/loop)
  --resize            Resize current image to match baseline size
  --update-baseline   Replace baseline with current after comparison
  --no-annotated      Skip generating annotated change-region image/spec
  --bbox-threshold <n>  Pixel diff threshold for bbox detection (default: 24)
  --bbox-min-area <n>   Min changed pixels per bbox (default: 64)
  --bbox-pad <n>        Padding around each bbox (default: 2)
  --max-boxes <n>       Maximum number of change boxes (default: 16)
  -h, --help          Show help

Behavior:
  - Stores latest, history, and diff images under the loop directory
  - Creates a baseline on first run
USAGE
}


# Backward-compat: if the legacy layout exists and the new one doesn't, keep using legacy by default.
if [[ -z "${CVLP_LOOP_DIR:-}" && -d "${out_root}/baselines" && ! -d "${out_root}/loop/baselines" ]]; then
  loop_dir="${out_root}"
fi
resize=0
update_baseline=0
emit_annotated=1
bbox_threshold=24
bbox_min_area=64
bbox_pad=2
max_boxes=16

while [[ $# -gt 0 ]]; do
  case "$1" in
    --loop-dir)
      loop_dir="$2"
      shift 2
      ;;
    --resize)
      resize=1
      shift
      ;;
    --update-baseline)
      update_baseline=1
      shift
      ;;
    --no-annotated)
      emit_annotated=0
      shift
      ;;
    --bbox-threshold)
      bbox_threshold="${2:-}"
      shift 2
      ;;
    --bbox-min-area)
      bbox_min_area="${2:-}"
      shift 2
      ;;
    --bbox-pad)
      bbox_pad="${2:-}"
      shift 2
      ;;
    --max-boxes)
      max_boxes="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      break
      ;;
  esac
done

current=${1:-}
baseline_name=${2:-}

if [[ -z "${current}" || -z "${baseline_name}" ]]; then
  usage >&2
  exit 1
fi

if [[ ! -f "${current}" ]]; then
  echo "error: current image not found: ${current}" >&2
  exit 1
fi

safe_name=$(echo "${baseline_name}" | tr ' /:' '___' | tr -cd 'A-Za-z0-9._-')
if [[ -z "${safe_name}" ]]; then
  safe_name="baseline"
fi

ts=$(date +%Y%m%d-%H%M%S)

base_dir="${loop_dir}"
base_baselines="${base_dir}/baselines"
base_latest="${base_dir}/latest"
base_history="${base_dir}/history"
base_diffs="${base_dir}/diffs"
base_reports="${base_dir}/reports"
base_annotations="${base_dir}/annotations"

mkdir -p "${base_baselines}" "${base_latest}" "${base_history}" "${base_diffs}" "${base_reports}" "${base_annotations}"

baseline_path="${base_baselines}/${safe_name}.png"
latest_path="${base_latest}/${safe_name}.png"
history_path="${base_history}/${safe_name}-${ts}.png"
diff_path="${base_diffs}/${safe_name}-${ts}.png"
json_path="${base_reports}/${safe_name}-${ts}.json"
annotated_path="${base_annotations}/${safe_name}-${ts}.png"
annotate_spec_path="${base_reports}/${safe_name}-${ts}-change-spec.json"

cp -f "${current}" "${latest_path}"
cp -f "${current}" "${history_path}"

if [[ ! -f "${baseline_path}" ]]; then
  cp -f "${current}" "${baseline_path}"
  BASELINE_PATH="${baseline_path}" LATEST_PATH="${latest_path}" HISTORY_PATH="${history_path}" \
    python3 - <<'PY'
import json
import os

print(
    json.dumps(
        {
            "baseline_created": os.path.abspath(os.environ["BASELINE_PATH"]),
            "latest": os.path.abspath(os.environ["LATEST_PATH"]),
            "history": os.path.abspath(os.environ["HISTORY_PATH"]),
        }
    )
)
PY
  exit 0
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

compare_args=(
  "${baseline_path}"
  "${current}"
  --diff-out "${diff_path}"
  --json-out "${json_path}"
  --bbox-threshold "${bbox_threshold}"
  --bbox-min-area "${bbox_min_area}"
  --bbox-pad "${bbox_pad}"
  --max-boxes "${max_boxes}"
)
if [[ ${resize} -eq 1 ]]; then
  compare_args+=(--resize)
fi
if [[ ${emit_annotated} -eq 1 ]]; then
  compare_args+=(--annotate-spec-out "${annotate_spec_path}")
fi

compare_output=$(python3 "${script_dir}/compare_images.py" "${compare_args[@]}")

if [[ ${emit_annotated} -eq 1 ]]; then
  if [[ -f "${annotate_spec_path}" ]]; then
    if python3 "${script_dir}/annotate_image.py" "${current}" "${annotated_path}" --spec "${annotate_spec_path}" --no-meta >/dev/null 2>&1; then
      updated_output=$(REPORT_PATH="${json_path}" INLINE_JSON="${compare_output}" ANNOTATED_PATH="${annotated_path}" SPEC_PATH="${annotate_spec_path}" \
        python3 - <<'PY'
import json
import os

report_path = os.environ.get("REPORT_PATH") or ""
inline_json = os.environ.get("INLINE_JSON") or "{}"
annotated_path = os.path.abspath(os.environ.get("ANNOTATED_PATH") or "")
spec_path = os.path.abspath(os.environ.get("SPEC_PATH") or "")

try:
    payload = json.loads(inline_json)
except json.JSONDecodeError:
    payload = {}

payload["annotated_image"] = annotated_path
payload["annotate_spec"] = spec_path

if report_path:
    with open(report_path, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2)

print(json.dumps(payload))
PY
      )
      compare_output="${updated_output}"
    else
      echo "warn: failed to render annotated diff image using annotate_image.py" >&2
    fi
  fi
fi

echo "${compare_output}"

if [[ ${update_baseline} -eq 1 ]]; then
  cp -f "${current}" "${baseline_path}"
fi
