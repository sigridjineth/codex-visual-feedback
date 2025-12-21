#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
observe_action_clip.sh

Capture before/after screenshots around an action, record a short clip, summarize frames, and emit one observation packet JSON.

Usage:
  observe_action_clip.sh [options]

Options:
  --process <name>        App process name to observe (default: frontmost app)
  --action <label>        Human-readable action label (default: observe)
  --action-cmd <cmd>      Optional shell command to execute between before/after capture
  --duration <sec>        Clip duration in seconds (default: 2)
  --out-dir <path>        Output directory (default: .codex-visual-loop/observe)
  --summary-mode <mode>   scene|fps|keyframes (default: scene)
  --summary-max <n>       Max summary frames (default: 16)
  --summary-sheet         Generate contact sheet (default: enabled)
  --summary-gif           Generate preview GIF (default: enabled)
  --no-summary            Skip clip frame summarization
  --json                  Print final packet JSON to stdout
  -h, --help              Show help

Env:
  CVLP_OUT_DIR override default output root (default: .codex-visual-loop)
USAGE
}

process=""
action_label="observe"
action_cmd=""
duration="2"
out_dir=""
run_summary=1
summary_mode="scene"
summary_max="16"
summary_sheet=1
summary_gif=1
print_json=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --process)
      process="${2:-}"
      shift 2
      ;;
    --action)
      action_label="${2:-}"
      shift 2
      ;;
    --action-cmd)
      action_cmd="${2:-}"
      shift 2
      ;;
    --duration)
      duration="${2:-}"
      shift 2
      ;;
    --out-dir)
      out_dir="${2:-}"
      shift 2
      ;;
    --summary-mode)
      summary_mode="${2:-}"
      shift 2
      ;;
    --summary-max)
      summary_max="${2:-}"
      shift 2
      ;;
    --summary-sheet)
      summary_sheet=1
      shift
      ;;
    --summary-gif)
      summary_gif=1
      shift
      ;;
    --no-summary)
      run_summary=0
      shift
      ;;
    --json)
      print_json=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown arg: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

out_dir=${out_dir:-"${out_root}/observe"}

if [[ -z "${process}" ]]; then
  process=$(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' 2>/dev/null || true)
fi

slug=$(echo "${process}" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9._-')
if [[ -z "${slug}" ]]; then
  slug="app"
fi

ts=$(date +%Y%m%d-%H%M%S)
run_id="${ts}-$$-$RANDOM"
mkdir -p "${out_dir}"

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

before_png="${out_dir}/before-${slug}-${run_id}.png"
after_png="${out_dir}/after-${slug}-${run_id}.png"
video_path="${out_dir}/action-clip-${slug}-${run_id}.mov"
action_log="${out_dir}/action-${slug}-${run_id}.log"
compare_json_path="${out_dir}/compare-${slug}-${run_id}.json"
diff_path="${out_dir}/diff-${slug}-${run_id}.png"
annotated_diff_path="${out_dir}/diff-annotated-${slug}-${run_id}.png"
annotate_spec_path="${out_dir}/diff-annotate-spec-${slug}-${run_id}.json"
report_path="${out_dir}/observe-${slug}-${run_id}.json"

before_capture_json=$(bash "${script_dir}/capture_app_window.sh" --out "${before_png}" --process "${process}" --step "before" --note "${action_label}" --json)

action_status=0
action_started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
if [[ -n "${action_cmd}" ]]; then
  set +e
  bash -lc "${action_cmd}" >"${action_log}" 2>&1
  action_status=$?
  set -e
else
  : >"${action_log}"
fi
action_finished=$(date -u +%Y-%m-%dT%H:%M:%SZ)

record_args=(--process "${process}" --duration "${duration}" --out "${video_path}" --json)
if [[ ${run_summary} -eq 1 ]]; then
  record_args+=(--summary --summary-mode "${summary_mode}" --summary-max "${summary_max}")
  if [[ ${summary_sheet} -eq 1 ]]; then
    record_args+=(--summary-sheet)
  fi
  if [[ ${summary_gif} -eq 1 ]]; then
    record_args+=(--summary-gif)
  fi
fi
clip_json=$(bash "${script_dir}/record_app_window.sh" "${record_args[@]}")

after_capture_json=$(bash "${script_dir}/capture_app_window.sh" --out "${after_png}" --process "${process}" --step "after" --note "${action_label}" --json)

compare_json=$(python3 "${script_dir}/compare_images.py" \
  "${before_png}" "${after_png}" \
  --diff-out "${diff_path}" \
  --json-out "${compare_json_path}" \
  --annotate-spec-out "${annotate_spec_path}")

if [[ -f "${annotate_spec_path}" ]]; then
  python3 "${script_dir}/annotate_image.py" "${after_png}" "${annotated_diff_path}" --spec "${annotate_spec_path}" --no-meta >/dev/null
fi

COMPARE_JSON_INLINE="${compare_json}" BEFORE_CAPTURE_JSON="${before_capture_json}" AFTER_CAPTURE_JSON="${after_capture_json}" \
CLIP_JSON="${clip_json}" REPORT_PATH="${report_path}" ACTION_LABEL="${action_label}" ACTION_CMD="${action_cmd}" ACTION_STATUS="${action_status}" \
ACTION_LOG="${action_log}" ACTION_STARTED="${action_started}" ACTION_FINISHED="${action_finished}" RUN_ID="${run_id}" PROCESS_NAME="${process}" \
DIFF_PATH="${diff_path}" ANNOTATED_DIFF_PATH="${annotated_diff_path}" ANNOTATE_SPEC_PATH="${annotate_spec_path}" \
python3 - <<'PY'
import json
import os


def _parse_json(raw: str):
    raw = raw or ""
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return None


compare_payload = _parse_json(os.environ.get("COMPARE_JSON_INLINE"))
if compare_payload is None:
    compare_payload = {}

annotated_diff_path = os.path.abspath(os.environ.get("ANNOTATED_DIFF_PATH") or "")
annotate_spec_path = os.path.abspath(os.environ.get("ANNOTATE_SPEC_PATH") or "")
if os.path.exists(annotated_diff_path):
    compare_payload["annotated_image"] = annotated_diff_path
if os.path.exists(annotate_spec_path):
    compare_payload["annotate_spec"] = annotate_spec_path

payload = {
    "run_id": os.environ.get("RUN_ID"),
    "process_name": os.environ.get("PROCESS_NAME") or None,
    "action": {
        "label": os.environ.get("ACTION_LABEL") or None,
        "command": os.environ.get("ACTION_CMD") or None,
        "status": int(os.environ.get("ACTION_STATUS") or "0"),
        "started_at": os.environ.get("ACTION_STARTED") or None,
        "finished_at": os.environ.get("ACTION_FINISHED") or None,
        "log_path": os.path.abspath(os.environ.get("ACTION_LOG") or ""),
    },
    "before_capture": _parse_json(os.environ.get("BEFORE_CAPTURE_JSON")) or {},
    "after_capture": _parse_json(os.environ.get("AFTER_CAPTURE_JSON")) or {},
    "clip": _parse_json(os.environ.get("CLIP_JSON")) or {},
    "diff": compare_payload,
}

report_path = os.path.abspath(os.environ["REPORT_PATH"])
os.makedirs(os.path.dirname(report_path), exist_ok=True)
with open(report_path, "w", encoding="utf-8") as f:
    json.dump(payload, f, indent=2)

print(report_path)
PY

if [[ ${print_json} -eq 1 ]]; then
  cat "${report_path}"
else
  echo "${report_path}"
  echo "${before_png}"
  echo "${after_png}"
  echo "${video_path}"
  echo "${annotated_diff_path}"
fi
