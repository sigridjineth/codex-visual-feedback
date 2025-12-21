#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
dump_ax_tree.sh

Dump an initial accessibility (AX) tree snapshot from a target app window.

Usage:
  dump_ax_tree.sh [--process <app_name>] [--depth <n>] [--out <tree.json>] [--json]

Options:
  --process <name>   App process name (default: frontmost app)
  --depth <n>        Traversal depth for UI element recursion (default: 3)
  --out <path>       Output JSON path (default: .codex-visual-loop/ax/ax-tree-<app>-<ts>-<pid>-<rand>.json)
  --json             Print tree JSON to stdout
  -h, --help         Show help

Env:
  CVLP_OUT_DIR override default output root (default: .codex-visual-loop)
USAGE
}

process=""
max_depth=3
out=""
print_json=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --process)
      process="${2:-}"
      shift 2
      ;;
    --depth)
      max_depth="${2:-}"
      shift 2
      ;;
    --out)
      out="${2:-}"
      shift 2
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

ax_dir="${out_root}/ax"
ts=$(date +%Y%m%d-%H%M%S)

if [[ -z "${process}" ]]; then
  process=$(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' 2>/dev/null || true)
fi

slug=$(echo "${process}" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9._-')
if [[ -z "${slug}" ]]; then
  slug="app"
fi

if [[ -z "${out}" ]]; then
  out="${ax_dir}/ax-tree-${slug}-${ts}-$$-$RANDOM.json"
fi

read -r -d '' APPLESCRIPT <<'APPLESCRIPT' || true
on sanitize(v)
  try
    set t to v as text
  on error
    set t to ""
  end try
  set AppleScript's text item delimiters to {return, linefeed, tab}
  set parts to text items of t
  set AppleScript's text item delimiters to " "
  set clean to parts as text
  set AppleScript's text item delimiters to ""
  return clean
end sanitize

on emitLine(depthVal, clsVal, nameVal, roleVal, enabledVal, xVal, yVal, wVal, hVal)
  return (depthVal as text) & tab & my sanitize(clsVal) & tab & my sanitize(nameVal) & tab & my sanitize(roleVal) & tab & my sanitize(enabledVal) & tab & (xVal as text) & tab & (yVal as text) & tab & (wVal as text) & tab & (hVal as text)
end emitLine

on walkNode(nodeRef, depthVal, maxDepth)
  set linesOut to {}

  try
    set clsVal to class of nodeRef as text
  on error
    set clsVal to "unknown"
  end try
  try
    set nameVal to name of nodeRef
  on error
    set nameVal to ""
  end try
  try
    set roleVal to role description of nodeRef
  on error
    set roleVal to ""
  end try
  try
    set enabledVal to enabled of nodeRef
  on error
    set enabledVal to ""
  end try
  set xVal to ""
  set yVal to ""
  set wVal to ""
  set hVal to ""
  try
    set posVal to position of nodeRef
    set xVal to item 1 of posVal
    set yVal to item 2 of posVal
  end try
  try
    set sizeVal to size of nodeRef
    set wVal to item 1 of sizeVal
    set hVal to item 2 of sizeVal
  end try

  set end of linesOut to my emitLine(depthVal, clsVal, nameVal, roleVal, enabledVal, xVal, yVal, wVal, hVal)

  if depthVal < maxDepth then
    try
      set childrenRefs to UI elements of nodeRef
      repeat with childRef in childrenRefs
        set childLines to my walkNode(childRef, depthVal + 1, maxDepth)
        set linesOut to linesOut & childLines
      end repeat
    end try
  end if

  return linesOut
end walkNode

on run argv
  set procName to item 1 of argv
  set depthLimit to item 2 of argv as integer
  tell application "System Events"
    tell process procName
      set targetWindow to window 1
      set linesOut to my walkNode(targetWindow, 0, depthLimit)
    end tell
  end tell
  set AppleScript's text item delimiters to linefeed
  set joined to linesOut as text
  set AppleScript's text item delimiters to ""
  return joined
end run
APPLESCRIPT

raw_lines=$(osascript -e "${APPLESCRIPT}" -- "${process}" "${max_depth}" 2>/dev/null || true)

if [[ -z "${raw_lines}" ]]; then
  echo "error: failed to read AX tree for process '${process}'" >&2
  echo "hint: verify Accessibility permissions and that a visible window exists" >&2
  exit 1
fi

RAW_AX_LINES="${raw_lines}" PROCESS_NAME="${process}" OUT_PATH="${out}" DEPTH_LIMIT="${max_depth}" \
python3 - <<'PY'
import json
import os
from datetime import datetime, timezone

raw = os.environ.get("RAW_AX_LINES") or ""
lines = [line for line in raw.splitlines() if line.strip()]

entries = []
for idx, line in enumerate(lines):
    parts = line.split("\t")
    while len(parts) < 9:
        parts.append("")
    depth_raw, cls, name, role_desc, enabled, x_raw, y_raw, w_raw, h_raw = parts[:9]
    try:
        depth = int(depth_raw)
    except ValueError:
        depth = 0
    def _to_int(value):
        try:
            return int(float(value))
        except Exception:
            return None
    x = _to_int(x_raw)
    y = _to_int(y_raw)
    w = _to_int(w_raw)
    h = _to_int(h_raw)
    entries.append(
        {
            "index": idx,
            "depth": depth,
            "class": cls,
            "name": name or None,
            "role_description": role_desc or None,
            "enabled": enabled if enabled != "" else None,
            "bounds": {
                "x": x,
                "y": y,
                "w": w,
                "h": h,
                "units": "pt",
            } if x is not None and y is not None and w is not None and h is not None else None,
        }
    )

root_nodes = []
stack = []
for entry in entries:
    node = {
        "index": entry["index"],
        "class": entry["class"],
        "name": entry["name"],
        "role_description": entry["role_description"],
        "enabled": entry["enabled"],
        "bounds": entry["bounds"],
        "children": [],
    }
    depth = int(entry["depth"])
    while stack and stack[-1]["depth"] >= depth:
        stack.pop()
    if stack:
        stack[-1]["node"]["children"].append(node)
    else:
        root_nodes.append(node)
    stack.append({"depth": depth, "node": node})

payload = {
    "captured_at": datetime.now(timezone.utc).isoformat(),
    "process_name": os.environ.get("PROCESS_NAME") or None,
    "depth_limit": int(os.environ.get("DEPTH_LIMIT") or 0),
    "element_count": len(entries),
    "elements": entries,
    "tree": root_nodes,
}

out_path = os.path.abspath(os.environ["OUT_PATH"])
os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, "w", encoding="utf-8") as f:
    json.dump(payload, f, indent=2)
PY

if [[ ${print_json} -eq 1 ]]; then
  python3 - <<'PY' "${out}"
import json
import os
import sys

with open(sys.argv[1], "r", encoding="utf-8") as f:
    payload = json.load(f)
print(json.dumps(payload))
PY
else
  echo "${out}"
fi
