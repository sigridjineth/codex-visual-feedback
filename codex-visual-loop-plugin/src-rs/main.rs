use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::{ArgAction, Args, Parser, Subcommand};
use font8x8::{UnicodeFonts, BASIC_FONTS};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageBuffer, Pixel, Rgba, RgbaImage};
use rand::Rng;
use serde_json::{json, Map, Value};
use std::env;
use std::f64::consts::PI;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use wait_timeout::ChildExt;

const PLUGIN_ROOT: &str = env!("CARGO_MANIFEST_DIR");

const SPEC_HELP: &str = r##"Spec JSON schema (minimal):
{
  "defaults": {
    "units": "px",
    "auto_scale": true,
    "outline": true,
    "auto_fit": true,
    "fit_mode": "luma",
    "fit_threshold": 160,
    "fit_target": "dark",
    "fit_min_pixels": 30,
    "fit_min_coverage": 0.6,
    "fit_pad": 0
  },
  "annotations": [
    {"type": "rect", "x": "10%", "y": "20%", "w": "35%", "h": "12%", "intent": "target", "action": "inspect", "color": "#FF3B30"},
    {"type": "arrow", "from": "cta", "to": "nearest", "color": "#0A84FF"},
    {"type": "text", "x": 130, "y": 90, "text": "Add button", "anchor": "cta", "color": "#FFFFFF"},
    {"type": "spotlight", "x": 110, "y": 70, "w": 190, "h": 60, "radius": 10}
  ]
}

Notes:
- auto-fit is enabled by default for rect/spotlight; disable with "fit": false or defaults.auto_fit=false.
- auto-fit snaps the original rect/spotlight to detected pixels (keeps size and recenters if detected area is smaller).
- coordinate fields accept px (default), "%" strings, and rel/fraction units via defaults.units="rel".
- anchor text/arrow endpoints via id/index/nearest with optional pos+offset.
- semantic fields like severity/issue/hypothesis/next_action/verify are preserved in metadata sidecars.
"##;

#[derive(Parser, Debug)]
#[command(
    name = "codex-visual-loop",
    version,
    about = "Rust CLI for codex-visual-loop-plugin visual capture/annotation/diff workflows"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::enum_variant_names)]
enum Commands {
    /// Print supported commands in JSON
    Commands,
    /// Print plugin manifest JSON
    Manifest,
    /// Capture app window (or fallback image) and emit metadata sidecar JSON
    Capture(CaptureArgs),
    /// Render annotation metadata/spec with semantic fields and relative units
    Annotate(AnnotateArgs),
    /// Compare baseline/current screenshots and emit diff-to-bbox outputs
    Diff(DiffArgs),
    /// Run baseline/history loop with diff reports and optional annotated output
    Loop(LoopArgs),
    /// Build one observation packet (before/after + action + clip + diff)
    Observe(ObserveArgs),
    /// Dump accessibility tree snapshot JSON
    #[command(name = "ax-tree")]
    AxTree(AxTreeArgs),
    /// Capture app + AX packet and optionally ask Codex CLI for a detailed explanation report
    #[command(name = "explain-app")]
    ExplainApp(ExplainArgs),
}

#[derive(Args, Debug)]
struct CaptureArgs {
    /// Output PNG path (positional fallback)
    out_path: Option<PathBuf>,
    /// Target process name (positional fallback)
    process_name: Option<String>,
    /// Output PNG path
    #[arg(long)]
    out: Option<PathBuf>,
    /// Target app process name
    #[arg(long)]
    process: Option<String>,
    /// Optional workflow step label (e.g. before/after)
    #[arg(long)]
    step: Option<String>,
    /// Optional free-form note stored in metadata
    #[arg(long)]
    note: Option<String>,
    /// Print capture metadata JSON to stdout
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    /// Custom metadata sidecar path (default: <out>.json)
    #[arg(long)]
    sidecar: Option<PathBuf>,
    /// Disable metadata sidecar generation
    #[arg(long, action = ArgAction::SetTrue)]
    no_sidecar: bool,
    /// Fail with non-zero status when capture falls back to generated placeholder output
    #[arg(long, action = ArgAction::SetTrue)]
    strict: bool,
}

#[derive(Args, Debug)]
struct AnnotateArgs {
    /// Input PNG path
    input: PathBuf,
    /// Output PNG path
    output: PathBuf,
    /// JSON spec file path (or - for stdin)
    #[arg(long)]
    spec: String,
    /// Path to write annotation metadata sidecar (default: <output>.json)
    #[arg(long)]
    meta_out: Option<PathBuf>,
    /// Disable metadata sidecar output
    #[arg(long, action = ArgAction::SetTrue)]
    no_meta: bool,
    /// Print spec schema and exit
    #[arg(long, action = ArgAction::SetTrue)]
    spec_help: bool,
}

#[derive(Args, Debug)]
struct DiffArgs {
    /// Path to baseline image
    baseline: PathBuf,
    /// Path to current image
    current: PathBuf,
    /// Path to write diff image (PNG)
    #[arg(long)]
    diff_out: Option<PathBuf>,
    /// Path to write JSON report
    #[arg(long)]
    json_out: Option<PathBuf>,
    /// Resize current to baseline size if dimensions differ
    #[arg(long, action = ArgAction::SetTrue)]
    resize: bool,
    /// Pixel diff threshold for bbox extraction
    #[arg(long, default_value_t = 24)]
    bbox_threshold: u8,
    /// Minimum changed pixels per region
    #[arg(long, default_value_t = 64)]
    bbox_min_area: u32,
    /// Padding around each bbox
    #[arg(long, default_value_t = 2)]
    bbox_pad: u32,
    /// Maximum number of change regions
    #[arg(long, default_value_t = 16)]
    max_boxes: usize,
    /// Path to write annotated current image with change boxes
    #[arg(long)]
    annotated_out: Option<PathBuf>,
    /// Path to write annotate-compatible JSON spec
    #[arg(long)]
    annotate_spec_out: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct LoopArgs {
    /// Current screenshot/image path
    current_path: PathBuf,
    /// Baseline key name
    baseline_name: String,
    /// Loop storage directory override
    #[arg(long)]
    loop_dir: Option<PathBuf>,
    /// Resize current to baseline size if dimensions differ
    #[arg(long, action = ArgAction::SetTrue)]
    resize: bool,
    /// Replace baseline with current after comparison
    #[arg(long, action = ArgAction::SetTrue)]
    update_baseline: bool,
    /// Skip generating annotated output/spec
    #[arg(long, action = ArgAction::SetTrue)]
    no_annotated: bool,
    /// Pixel diff threshold for bbox extraction
    #[arg(long, default_value_t = 24)]
    bbox_threshold: u8,
    /// Minimum changed pixels per region
    #[arg(long, default_value_t = 64)]
    bbox_min_area: u32,
    /// Padding around each bbox
    #[arg(long, default_value_t = 2)]
    bbox_pad: u32,
    /// Maximum number of change regions
    #[arg(long, default_value_t = 16)]
    max_boxes: usize,
}

#[derive(Args, Debug)]
struct ObserveArgs {
    /// App process name to observe (default: frontmost app)
    #[arg(long)]
    process: Option<String>,
    /// Human-readable action label
    #[arg(long, default_value = "observe")]
    action: String,
    /// Optional shell command to execute between before/after capture
    #[arg(long)]
    action_cmd: Option<String>,
    /// Clip duration in seconds
    #[arg(long, default_value_t = 2)]
    duration: u64,
    /// Output directory (default: .codex-visual-loop/observe)
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// scene|fps|keyframes
    #[arg(long, default_value = "scene")]
    summary_mode: String,
    /// Max summary frames
    #[arg(long, default_value_t = 16)]
    summary_max: u32,
    /// Generate contact sheet metadata flag
    #[arg(long, action = ArgAction::SetTrue)]
    summary_sheet: bool,
    /// Generate preview gif metadata flag
    #[arg(long, action = ArgAction::SetTrue)]
    summary_gif: bool,
    /// Skip clip summary generation
    #[arg(long, action = ArgAction::SetTrue)]
    no_summary: bool,
    /// Print final observation packet JSON to stdout
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Args, Debug)]
struct AxTreeArgs {
    /// App process name (default: frontmost app)
    #[arg(long)]
    process: Option<String>,
    /// Traversal depth for accessibility recursion
    #[arg(long, default_value_t = 3)]
    depth: u32,
    /// Output JSON path
    #[arg(long)]
    out: Option<PathBuf>,
    /// Print tree JSON to stdout
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Args, Debug)]
struct ExplainArgs {
    /// App process name (default: frontmost app)
    #[arg(long)]
    process: Option<String>,
    /// AX traversal depth
    #[arg(long, default_value_t = 4)]
    ax_depth: u32,
    /// Artifact root (default: CVLP_OUT_DIR / .codex-visual-loop)
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// Extra instructions appended to prompt
    #[arg(long)]
    prompt: Option<String>,
    /// Custom markdown report output path
    #[arg(long)]
    report: Option<PathBuf>,
    /// Custom packet JSON output path
    #[arg(long)]
    packet_out: Option<PathBuf>,
    /// Custom prompt text output path
    #[arg(long)]
    prompt_out: Option<PathBuf>,
    /// Override Codex executable path
    #[arg(long)]
    codex_bin: Option<String>,
    /// Optional Codex model override
    #[arg(long)]
    model: Option<String>,
    /// Timeout seconds for codex exec
    #[arg(long, default_value_t = 300)]
    codex_timeout: u64,
    /// Skip codex exec and emit fallback markdown report
    #[arg(long, action = ArgAction::SetTrue)]
    no_codex: bool,
    /// Exit non-zero if codex exec fails (instead of fallback report)
    #[arg(long, action = ArgAction::SetTrue)]
    strict_llm: bool,
    /// Emit JSON payload
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ChangeRegion {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    x2: u32,
    y2: u32,
    pixels: u32,
    area: u32,
    coverage: f64,
    intent: String,
    action: String,
    id: String,
    rel: RegionRel,
}

#[derive(Debug, Clone, serde::Serialize)]
struct RegionRel {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct QueryDiagnostic {
    ok: bool,
    attempts: u32,
    error_code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Clone)]
struct WindowProbe {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    title: Option<String>,
    selected_index: Option<usize>,
    candidate_count: usize,
    usable_count: usize,
    selection_mode: String,
    usable: bool,
    min_width: i64,
    min_height: i64,
    min_area: i64,
    diagnostics: QueryDiagnostic,
}

#[derive(Debug, Clone)]
struct WindowCandidate {
    index: usize,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    title: Option<String>,
}

#[derive(Debug, Clone)]
struct AnchorTarget {
    id: Option<String>,
    index: usize,
    ann_type: String,
    bbox: (f64, f64, f64, f64),
}

#[derive(Debug, Clone)]
struct AnchorSpec {
    id: Option<String>,
    index: Option<usize>,
    nearest: bool,
    target_type: Option<String>,
    pos: Option<String>,
    offset: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
struct AxFlatNode {
    index: usize,
    depth: usize,
    class_name: String,
    name: Option<String>,
    role_description: Option<String>,
    enabled: Option<String>,
    bounds: Option<(i64, i64, i64, i64)>,
}

#[derive(Debug, Clone)]
struct AxTreeNode {
    index: usize,
    class_name: String,
    name: Option<String>,
    role_description: Option<String>,
    enabled: Option<String>,
    bounds: Option<(i64, i64, i64, i64)>,
    children: Vec<AxTreeNode>,
}

#[derive(Debug, Clone)]
struct AxQueryResult {
    elements: Vec<Value>,
    tree: Vec<Value>,
    diagnostics: QueryDiagnostic,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct DiffRunOutput {
    json: Value,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Commands => print_commands(),
        Commands::Manifest => print_manifest(),
        Commands::Capture(args) => command_capture(args),
        Commands::Annotate(args) => command_annotate(args),
        Commands::Diff(args) => command_diff(args),
        Commands::Loop(args) => command_loop(args),
        Commands::Observe(args) => command_observe(args),
        Commands::AxTree(args) => command_ax_tree(args),
        Commands::ExplainApp(args) => command_explain_app(args),
    }
}

fn print_commands() -> Result<()> {
    let rows = vec![
        json!({
            "name": "capture",
            "description": "Capture an app window and emit metadata JSON sidecars.",
            "runner": "rust"
        }),
        json!({
            "name": "annotate",
            "description": "Render annotation specs with semantic fields + relative units.",
            "runner": "rust"
        }),
        json!({
            "name": "diff",
            "description": "Compare screenshots and emit diff-to-bbox annotation specs.",
            "runner": "rust"
        }),
        json!({
            "name": "loop",
            "description": "Run baseline/history diff loops with auto-annotated change boxes.",
            "runner": "rust"
        }),
        json!({
            "name": "observe",
            "description": "Build observation packet JSON (before/after/clip/diff).",
            "runner": "rust"
        }),
        json!({
            "name": "ax-tree",
            "description": "Dump accessibility tree snapshots for UI grounding.",
            "runner": "rust"
        }),
        json!({
            "name": "explain-app",
            "description": "Capture + AX packet and optional Codex exec report generation.",
            "runner": "rust"
        }),
    ];

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "commands": rows }))?
    );
    Ok(())
}

fn print_manifest() -> Result<()> {
    let manifest_path = Path::new(PLUGIN_ROOT).join("manifest.json");
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("manifest not found: {}", manifest_path.display()))?;
    let payload: Value = serde_json::from_str(&raw)
        .with_context(|| format!("invalid manifest JSON: {}", manifest_path.display()))?;
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn command_capture(args: CaptureArgs) -> Result<()> {
    let process = args
        .process
        .clone()
        .or_else(|| args.process_name.clone())
        .or_else(frontmost_app_name);

    let out_root = out_root();
    let captures_dir = out_root.join("capture");

    let resolved_out = args
        .out
        .clone()
        .or_else(|| args.out_path.clone())
        .unwrap_or_else(|| {
            let slug = slugify(process.as_deref().unwrap_or("app"));
            let ts = timestamp_compact();
            let rand = rand::thread_rng().gen_range(1000..9999);
            captures_dir.join(format!(
                "app-window-{slug}-{ts}-{}-{rand}.png",
                std::process::id()
            ))
        });

    let sidecar_path = if args.no_sidecar {
        None
    } else {
        Some(
            args.sidecar
                .unwrap_or_else(|| default_sidecar_for(&resolved_out)),
        )
    };

    let payload = capture_internal(
        &resolved_out,
        process.clone(),
        args.step.as_deref(),
        args.note.as_deref(),
        sidecar_path.as_deref(),
    )?;

    let fallback_used = payload
        .get("fallback_used")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if args.strict && fallback_used {
        if args.json {
            println!("{}", serde_json::to_string(&payload)?);
        }
        bail!(
            "capture fell back to placeholder output. Check Screen Recording/Accessibility permissions or retry with a visible app window."
        );
    }

    if args.json {
        println!("{}", serde_json::to_string(&payload)?);
    } else {
        let output_path = payload
            .get("image_path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        println!("{output_path}");
    }

    Ok(())
}

fn command_annotate(args: AnnotateArgs) -> Result<()> {
    if args.spec_help {
        println!("{}", SPEC_HELP.trim());
        return Ok(());
    }

    if !args.input.exists() {
        bail!("input not found: {}", args.input.display());
    }

    let spec = load_spec(&args.spec)?;
    let defaults = spec
        .get("defaults")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let annotations = spec
        .get("annotations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let input_image = image::open(&args.input)
        .with_context(|| format!("failed to open input image: {}", args.input.display()))?;
    let mut rendered = input_image.to_rgba8();
    let fit_image = input_image.to_rgb8();
    let (img_w, img_h) = rendered.dimensions();
    let base_scale = resolve_scale(&defaults, img_w, img_h);

    let mut prepared_spotlights: Vec<(usize, Map<String, Value>)> = Vec::new();
    let mut prepared_others: Vec<(usize, Map<String, Value>)> = Vec::new();

    for (idx, ann) in annotations.iter().enumerate() {
        let ann_obj = match ann.as_object() {
            Some(obj) => obj,
            None => continue,
        };

        let mut merged = defaults.clone();
        for (k, v) in ann_obj {
            merged.insert(k.clone(), v.clone());
        }

        resolve_annotation_units(&mut merged, img_w, img_h, &defaults);
        let ann_type = annotation_type(&merged);
        if is_spotlight_type(&ann_type) {
            let fitted = apply_fit(&merged, &fit_image, img_w, img_h, &defaults);
            prepared_spotlights.push((idx, fitted));
        } else {
            prepared_others.push((idx, merged));
        }
    }

    let mut anchor_targets: Vec<AnchorTarget> = Vec::new();
    for (idx, ann) in &prepared_spotlights {
        if let Some(bbox) = bbox_from_ann(ann) {
            anchor_targets.push(AnchorTarget {
                id: ann
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                index: *idx,
                ann_type: "spotlight".to_string(),
                bbox,
            });
        }
    }

    let mut prepared_render_list: Vec<(usize, Map<String, Value>)> = Vec::new();
    for (idx, mut ann) in prepared_others {
        let ann_type = annotation_type(&ann);
        if ann_type == "rect" {
            ann = apply_fit(&ann, &fit_image, img_w, img_h, &defaults);
            if let Some(bbox) = bbox_from_ann(&ann) {
                anchor_targets.push(AnchorTarget {
                    id: ann
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    index: idx,
                    ann_type: "rect".to_string(),
                    bbox,
                });
            }
        }
        prepared_render_list.push((idx, ann));
    }

    let mut processed_meta: Vec<Value> = Vec::new();

    for (idx, ann) in &prepared_spotlights {
        let scale = ann
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(base_scale);
        draw_spotlight_annotation(&mut rendered, ann, scale, &defaults);
        processed_meta.push(annotation_meta_item(*idx, ann, img_w, img_h));
    }

    for (idx, ann) in prepared_render_list {
        let ann_type = annotation_type(&ann);
        let scale = ann
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(base_scale);
        let mut rendered_ann = ann.clone();

        match ann_type.as_str() {
            "rect" => draw_rect_annotation(&mut rendered, &rendered_ann, scale),
            "arrow" => {
                rendered_ann =
                    apply_arrow_anchor(&rendered_ann, &anchor_targets, &defaults, img_w, img_h);
                draw_arrow_annotation(&mut rendered, &rendered_ann, scale);
            }
            "text" => {
                rendered_ann =
                    apply_text_anchor(&rendered_ann, &anchor_targets, &defaults, img_w, img_h);
                draw_text_annotation(&mut rendered, &rendered_ann, scale);
            }
            _ => {}
        }

        processed_meta.push(annotation_meta_item(idx, &rendered_ann, img_w, img_h));
    }

    processed_meta.sort_by_key(|item| item.get("index").and_then(Value::as_u64).unwrap_or(0));

    ensure_parent_dir(&args.output)?;
    DynamicImage::ImageRgba8(rendered)
        .save(&args.output)
        .with_context(|| format!("failed to save output image: {}", args.output.display()))?;

    if !args.no_meta {
        let meta_path = args
            .meta_out
            .clone()
            .unwrap_or_else(|| default_sidecar_for(&args.output));
        ensure_parent_dir(&meta_path)?;

        let payload = json!({
            "annotation_meta_version": 1,
            "input_path": abs_path(&args.input).display().to_string(),
            "output_path": abs_path(&args.output).display().to_string(),
            "meta_path": abs_path(&meta_path).display().to_string(),
            "generated_at": timestamp_iso(),
            "size": {"width": img_w, "height": img_h, "units": "px"},
            "defaults": Value::Object(defaults),
            "annotations": processed_meta,
        });

        write_json_pretty(&meta_path, &payload)?;
    }

    println!("{}", abs_path(&args.output).display());
    Ok(())
}

fn command_diff(args: DiffArgs) -> Result<()> {
    let output = run_diff_internal(
        &args.baseline,
        &args.current,
        args.diff_out.as_deref(),
        args.json_out.as_deref(),
        args.resize,
        args.bbox_threshold,
        args.bbox_min_area,
        args.bbox_pad,
        args.max_boxes,
        args.annotated_out.as_deref(),
        args.annotate_spec_out.as_deref(),
    )?;

    println!("{}", serde_json::to_string(&output.json)?);
    Ok(())
}

fn command_loop(args: LoopArgs) -> Result<()> {
    if !args.current_path.exists() {
        bail!("current image not found: {}", args.current_path.display());
    }

    let out_root = out_root();
    let legacy_baselines = out_root.join("baselines");
    let new_baselines = out_root.join("loop").join("baselines");

    let mut loop_dir = args
        .loop_dir
        .clone()
        .or_else(|| env::var("CVLP_LOOP_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| out_root.join("loop"));

    if args.loop_dir.is_none()
        && env::var("CVLP_LOOP_DIR").is_err()
        && legacy_baselines.exists()
        && !new_baselines.exists()
    {
        loop_dir = out_root;
    }

    let safe_name = sanitize_baseline_name(&args.baseline_name);
    let ts = timestamp_compact();

    let base_baselines = loop_dir.join("baselines");
    let base_latest = loop_dir.join("latest");
    let base_history = loop_dir.join("history");
    let base_diffs = loop_dir.join("diffs");
    let base_reports = loop_dir.join("reports");
    let base_annotations = loop_dir.join("annotations");

    for dir in [
        &base_baselines,
        &base_latest,
        &base_history,
        &base_diffs,
        &base_reports,
        &base_annotations,
    ] {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create loop dir: {}", dir.display()))?;
    }

    let baseline_path = base_baselines.join(format!("{safe_name}.png"));
    let latest_path = base_latest.join(format!("{safe_name}.png"));
    let history_path = base_history.join(format!("{safe_name}-{ts}.png"));
    let diff_path = base_diffs.join(format!("{safe_name}-{ts}.png"));
    let json_path = base_reports.join(format!("{safe_name}-{ts}.json"));
    let annotated_path = base_annotations.join(format!("{safe_name}-{ts}.png"));
    let annotate_spec_path = base_reports.join(format!("{safe_name}-{ts}-change-spec.json"));

    copy_file(&args.current_path, &latest_path)?;
    copy_file(&args.current_path, &history_path)?;

    if !baseline_path.exists() {
        copy_file(&args.current_path, &baseline_path)?;
        let payload = json!({
            "baseline_created": abs_path(&baseline_path).display().to_string(),
            "latest": abs_path(&latest_path).display().to_string(),
            "history": abs_path(&history_path).display().to_string(),
        });
        println!("{}", serde_json::to_string(&payload)?);
        return Ok(());
    }

    let emit_annotated = !args.no_annotated;
    let diff_output = run_diff_internal(
        &baseline_path,
        &args.current_path,
        Some(&diff_path),
        Some(&json_path),
        args.resize,
        args.bbox_threshold,
        args.bbox_min_area,
        args.bbox_pad,
        args.max_boxes,
        if emit_annotated {
            Some(&annotated_path)
        } else {
            None
        },
        if emit_annotated {
            Some(&annotate_spec_path)
        } else {
            None
        },
    )?;

    if args.update_baseline {
        copy_file(&args.current_path, &baseline_path)?;
    }

    println!("{}", serde_json::to_string(&diff_output.json)?);
    Ok(())
}

fn command_observe(args: ObserveArgs) -> Result<()> {
    let process = args
        .process
        .clone()
        .or_else(frontmost_app_name)
        .unwrap_or_else(|| "app".to_string());

    let out_root = out_root();
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| out_root.join("observe"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create observe dir: {}", out_dir.display()))?;

    let slug = slugify(&process);
    let run_id = format!(
        "{}-{}-{}",
        timestamp_compact(),
        std::process::id(),
        rand::thread_rng().gen_range(1000..9999)
    );

    let before_png = out_dir.join(format!("before-{slug}-{run_id}.png"));
    let after_png = out_dir.join(format!("after-{slug}-{run_id}.png"));
    let video_path = out_dir.join(format!("action-clip-{slug}-{run_id}.mov"));
    let action_log = out_dir.join(format!("action-{slug}-{run_id}.log"));
    let compare_json_path = out_dir.join(format!("compare-{slug}-{run_id}.json"));
    let diff_path = out_dir.join(format!("diff-{slug}-{run_id}.png"));
    let annotated_diff_path = out_dir.join(format!("diff-annotated-{slug}-{run_id}.png"));
    let annotate_spec_path = out_dir.join(format!("diff-annotate-spec-{slug}-{run_id}.json"));
    let report_path = out_dir.join(format!("observe-{slug}-{run_id}.json"));

    let before_payload = capture_internal(
        &before_png,
        Some(process.clone()),
        Some("before"),
        Some(&args.action),
        Some(&default_sidecar_for(&before_png)),
    )?;

    let action_started = timestamp_iso();
    let action_status = if let Some(cmd) = args.action_cmd.as_deref() {
        let output = Command::new("bash")
            .arg("-lc")
            .arg(cmd)
            .output()
            .with_context(|| format!("failed to run action command: {cmd}"))?;
        let mut file = File::create(&action_log)
            .with_context(|| format!("failed to write action log: {}", action_log.display()))?;
        file.write_all(&output.stdout)?;
        file.write_all(&output.stderr)?;
        output.status.code().unwrap_or(1)
    } else {
        File::create(&action_log)
            .with_context(|| format!("failed to create action log: {}", action_log.display()))?;
        0
    };
    let action_finished = timestamp_iso();

    let mut clip_file = File::create(&video_path).with_context(|| {
        format!(
            "failed to create clip placeholder: {}",
            video_path.display()
        )
    })?;
    clip_file.write_all(b"codex-visual-loop placeholder clip\n")?;

    if args.duration > 0 {
        thread::sleep(Duration::from_secs(args.duration.min(30)));
    }

    let after_payload = capture_internal(
        &after_png,
        Some(process.clone()),
        Some("after"),
        Some(&args.action),
        Some(&default_sidecar_for(&after_png)),
    )?;

    let diff_output = run_diff_internal(
        &before_png,
        &after_png,
        Some(&diff_path),
        Some(&compare_json_path),
        true,
        24,
        16,
        2,
        16,
        Some(&annotated_diff_path),
        Some(&annotate_spec_path),
    )?;

    let clip_payload = json!({
        "video_path": abs_path(&video_path).display().to_string(),
        "duration_sec": args.duration,
        "summary_mode": args.summary_mode,
        "summary_max": args.summary_max,
        "summary_enabled": !args.no_summary,
        "summary_sheet": args.summary_sheet,
        "summary_gif": args.summary_gif,
    });

    let payload = json!({
        "run_id": run_id,
        "process_name": process,
        "action": {
            "label": args.action,
            "command": args.action_cmd,
            "status": action_status,
            "started_at": action_started,
            "finished_at": action_finished,
            "log_path": abs_path(&action_log).display().to_string(),
        },
        "before_capture": before_payload,
        "after_capture": after_payload,
        "clip": clip_payload,
        "diff": diff_output.json,
    });

    write_json_pretty(&report_path, &payload)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", abs_path(&report_path).display());
        println!("{}", abs_path(&before_png).display());
        println!("{}", abs_path(&after_png).display());
        println!("{}", abs_path(&video_path).display());
        println!("{}", abs_path(&annotated_diff_path).display());
    }

    Ok(())
}

fn command_ax_tree(args: AxTreeArgs) -> Result<()> {
    let process = args
        .process
        .clone()
        .or_else(frontmost_app_name)
        .unwrap_or_else(|| "app".to_string());

    let slug = slugify(&process);
    let ts = timestamp_compact();
    let out_root = out_root();
    let out = args.out.clone().unwrap_or_else(|| {
        out_root.join("ax").join(format!(
            "ax-tree-{slug}-{ts}-{}-{}.json",
            std::process::id(),
            rand::thread_rng().gen_range(1000..9999)
        ))
    });
    let ax = query_ax_tree(&process, args.depth.max(1));

    let payload = json!({
        "captured_at": timestamp_iso(),
        "process_name": process,
        "depth_limit": args.depth,
        "element_count": ax.elements.len(),
        "elements": ax.elements,
        "tree": ax.tree,
        "query": ax.diagnostics,
        "warnings": ax.warnings,
    });

    write_json_pretty(&out, &payload)?;

    if args.json {
        println!("{}", serde_json::to_string(&payload)?);
    } else {
        println!("{}", abs_path(&out).display());
    }

    Ok(())
}

fn command_explain_app(args: ExplainArgs) -> Result<()> {
    let process = args
        .process
        .clone()
        .or_else(frontmost_app_name)
        .unwrap_or_else(|| "app".to_string());

    let out_root = args.out_dir.clone().unwrap_or_else(out_root);
    let explain_dir = out_root.join("explain");
    fs::create_dir_all(&explain_dir)
        .with_context(|| format!("failed to create explain dir: {}", explain_dir.display()))?;

    let slug = slugify(&process);
    let run_id = format!(
        "{}-{}-{}",
        timestamp_compact(),
        std::process::id(),
        rand::thread_rng().gen_range(1000..9999)
    );
    let base = format!("explain-{slug}-{run_id}");

    let image_path = explain_dir.join(format!("{base}-capture.png"));
    let packet_path = args
        .packet_out
        .clone()
        .unwrap_or_else(|| explain_dir.join(format!("{base}-packet.json")));
    let prompt_path = args
        .prompt_out
        .clone()
        .unwrap_or_else(|| explain_dir.join(format!("{base}-prompt.txt")));
    let report_path = args
        .report
        .clone()
        .unwrap_or_else(|| explain_dir.join(format!("{base}-report.md")));
    let codex_log_path = explain_dir.join(format!("{base}-codex.log"));

    let capture = capture_internal(
        &image_path,
        Some(process.clone()),
        Some("explain"),
        Some("explain-app"),
        Some(&default_sidecar_for(&image_path)),
    )?;
    let ax = query_ax_tree(&process, args.ax_depth.max(1));
    let summary = summarize_ax_elements(&ax.elements);

    let mut warnings = Vec::<String>::new();
    if let Some(items) = capture.get("warnings").and_then(Value::as_array) {
        for item in items {
            if let Some(text) = item.as_str() {
                warnings.push(text.to_string());
            }
        }
    }
    for warning in &ax.warnings {
        warnings.push(warning.clone());
    }

    let ax_payload = json!({
        "captured_at": timestamp_iso(),
        "process_name": process,
        "depth_limit": args.ax_depth,
        "element_count": ax.elements.len(),
        "elements": ax.elements,
        "tree": ax.tree,
        "query": ax.diagnostics,
        "warnings": ax.warnings,
    });

    let packet = json!({
        "packet_version": 1,
        "generated_at": timestamp_iso(),
        "process_name": process,
        "capture": capture,
        "ax_tree": ax_payload,
        "summary": summary,
        "warnings": warnings,
    });

    write_json_pretty(&packet_path, &packet)?;
    let prompt_text = build_explain_prompt(&packet, args.prompt.as_deref());
    write_text_file(&prompt_path, &prompt_text)?;

    let mut mode = "fallback".to_string();
    let mut fallback_reason: Option<String> = None;
    let mut codex_meta = json!({
        "attempted": false,
        "success": false,
    });

    if args.no_codex {
        fallback_reason = Some("Codex execution disabled via --no-codex".to_string());
    } else if let Some(codex_bin) = resolve_codex_executable(args.codex_bin.as_deref()) {
        codex_meta = run_codex_exec_report(
            &codex_bin,
            &prompt_text,
            &image_path,
            &report_path,
            &codex_log_path,
            args.model.as_deref(),
            args.codex_timeout,
        )?;

        if codex_meta
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            mode = "llm".to_string();
        } else {
            fallback_reason = Some(
                codex_meta
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("codex exec did not produce a usable report")
                    .to_string(),
            );
        }
    } else {
        fallback_reason = Some("Codex CLI not found on PATH (`codex`/`codex-auto`)".to_string());
    }

    if mode != "llm" {
        if args.strict_llm && !args.no_codex {
            let payload = json!({
                "mode": "error",
                "error": fallback_reason,
                "packet_path": abs_path(&packet_path).display().to_string(),
                "prompt_path": abs_path(&prompt_path).display().to_string(),
                "report_path": abs_path(&report_path).display().to_string(),
                "codex": codex_meta,
            });
            if args.json {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            bail!(
                "{}",
                payload
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("strict_llm enabled and codex execution failed")
            );
        }

        let fallback_markdown = build_fallback_explain_report(
            &packet,
            fallback_reason
                .as_deref()
                .unwrap_or("codex execution unavailable"),
            &codex_meta,
        );
        write_text_file(&report_path, &fallback_markdown)?;
    }

    let output = json!({
        "mode": mode,
        "process_name": process,
        "packet_path": abs_path(&packet_path).display().to_string(),
        "prompt_path": abs_path(&prompt_path).display().to_string(),
        "report_path": abs_path(&report_path).display().to_string(),
        "codex": codex_meta,
        "fallback_reason": fallback_reason,
    });

    if args.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", abs_path(&report_path).display());
    }

    Ok(())
}

fn summarize_ax_elements(elements: &[Value]) -> Value {
    let mut named_elements = 0usize;
    let mut interactive_guess_count = 0usize;
    let mut role_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    for element in elements {
        let Some(obj) = element.as_object() else {
            continue;
        };
        if obj
            .get("name")
            .and_then(Value::as_str)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        {
            named_elements += 1;
        }

        let role = obj
            .get("role_description")
            .and_then(Value::as_str)
            .or_else(|| obj.get("class").and_then(Value::as_str))
            .unwrap_or("unknown")
            .trim()
            .to_string();
        *role_counts.entry(role.clone()).or_insert(0usize) += 1;

        let haystack = format!(
            "{} {}",
            role.to_ascii_lowercase(),
            obj.get("class")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase()
        );
        if [
            "button", "checkbox", "menu", "tab", "slider", "text", "field", "link",
        ]
        .iter()
        .any(|token| haystack.contains(token))
        {
            interactive_guess_count += 1;
        }
    }

    let mut top_roles: Vec<(String, usize)> = role_counts.into_iter().collect();
    top_roles.sort_by(|a, b| b.1.cmp(&a.1));
    top_roles.truncate(8);

    json!({
        "element_count": elements.len(),
        "named_elements": named_elements,
        "interactive_guess_count": interactive_guess_count,
        "top_roles": top_roles.into_iter().map(|(role, count)| json!({"role": role, "count": count})).collect::<Vec<Value>>(),
    })
}

fn build_explain_prompt(packet: &Value, extra_prompt: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(
        "You are analyzing one screenshot of a macOS app and companion accessibility metadata.\n",
    );
    out.push_str("Write a detailed UI explanation report in Markdown.\n\n");
    out.push_str("Required sections:\n");
    out.push_str("1) Executive summary\n");
    out.push_str("2) Visual/layout walkthrough (top-to-bottom)\n");
    out.push_str("3) Interactive elements and likely affordances\n");
    out.push_str("4) Accessibility and usability observations\n");
    out.push_str("5) Risks/unknowns\n");
    out.push_str("6) Recommended next actions\n\n");
    out.push_str("If something is uncertain, explicitly say so.\n");
    if let Some(extra) = extra_prompt {
        if !extra.trim().is_empty() {
            out.push_str("\nExtra user instructions:\n");
            out.push_str(extra.trim());
            out.push('\n');
        }
    }
    out.push_str("\nContext packet JSON:\n");
    out.push_str(&serde_json::to_string_pretty(packet).unwrap_or_else(|_| "{}".to_string()));
    out.push('\n');
    out
}

fn build_fallback_explain_report(packet: &Value, reason: &str, codex_meta: &Value) -> String {
    let summary = packet.get("summary").cloned().unwrap_or_else(|| json!({}));
    let capture = packet.get("capture").cloned().unwrap_or_else(|| json!({}));

    let mut lines = vec![
        "# App Explanation Report (fallback)".to_string(),
        "".to_string(),
        format!(
            "- Generated at: {}",
            packet
                .get("generated_at")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
        format!(
            "- Process: {}",
            packet
                .get("process_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
        format!(
            "- Screenshot: {}",
            capture
                .get("capture_path")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
        "".to_string(),
        "## Why fallback mode was used".to_string(),
        reason.to_string(),
        "".to_string(),
        "## AX summary".to_string(),
        format!(
            "- Element count: {}",
            summary
                .get("element_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "- Named elements: {}",
            summary
                .get("named_elements")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "- Interactive guess count: {}",
            summary
                .get("interactive_guess_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
    ];

    if let Some(roles) = summary.get("top_roles").and_then(Value::as_array) {
        lines.push("- Top roles:".to_string());
        for role in roles {
            lines.push(format!(
                "  - {}: {}",
                role.get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                role.get("count").and_then(Value::as_u64).unwrap_or(0)
            ));
        }
    }

    if let Some(error) = codex_meta.get("error").and_then(Value::as_str) {
        lines.push("".to_string());
        lines.push("## Codex attempt".to_string());
        lines.push(format!("- Error: {error}"));
    }

    lines.join("\n") + "\n"
}

fn resolve_codex_executable(override_bin: Option<&str>) -> Option<String> {
    if let Some(bin) = override_bin {
        let trimmed = bin.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if command_exists("codex") {
        return Some("codex".to_string());
    }
    if command_exists("codex-auto") {
        return Some("codex-auto".to_string());
    }
    None
}

fn run_codex_exec_report(
    codex_bin: &str,
    prompt_text: &str,
    image_path: &Path,
    report_path: &Path,
    log_path: &Path,
    model: Option<&str>,
    timeout_sec: u64,
) -> Result<Value> {
    let mut cmd = Command::new(codex_bin);
    cmd.arg("exec")
        .arg("--output-last-message")
        .arg(report_path)
        .arg("--image")
        .arg(image_path);
    if let Some(model) = model {
        cmd.arg("--model").arg(model);
    }
    cmd.arg("-");
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            return Ok(json!({
                "attempted": true,
                "success": false,
                "command": format!("{codex_bin} exec ..."),
                "error": err.to_string(),
            }));
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt_text.as_bytes());
    }

    let timeout = Duration::from_secs(timeout_sec.max(10));
    let status_opt = child.wait_timeout(timeout).map_err(anyhow::Error::from)?;
    if status_opt.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(json!({
            "attempted": true,
            "success": false,
            "command": format!("{codex_bin} exec ..."),
            "error": format!("codex exec timed out after {}s", timeout.as_secs()),
        }));
    }

    let output = child
        .wait_with_output()
        .with_context(|| "failed to read codex exec output")?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let log_text = format!(
        "command: {codex_bin} exec --output-last-message {} --image {} -\nreturncode: {}\n\nstdout:\n{}\n\nstderr:\n{}\n",
        report_path.display(),
        image_path.display(),
        output.status.code().unwrap_or(1),
        stdout,
        stderr
    );
    write_text_file(log_path, &log_text)?;

    let report_text = fs::read_to_string(report_path).unwrap_or_default();
    let success = output.status.success() && !report_text.trim().is_empty();
    Ok(json!({
        "attempted": true,
        "success": success,
        "returncode": output.status.code().unwrap_or(1),
        "report_path": abs_path(report_path).display().to_string(),
        "log_path": abs_path(log_path).display().to_string(),
        "stdout_tail": truncate_text(&stdout, 2400),
        "stderr_tail": truncate_text(&stderr, 2400),
        "error": if success { Value::Null } else { json!("codex exec did not produce a usable report") },
    }))
}

fn truncate_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect::<String>() + "\n...<truncated>..."
}

fn write_text_file(path: &Path, content: &str) -> Result<()> {
    ensure_parent_dir(path)?;
    fs::write(path, content).with_context(|| format!("failed to write text: {}", path.display()))
}

#[allow(clippy::too_many_arguments)]
fn run_diff_internal(
    baseline_path: &Path,
    current_path: &Path,
    diff_out: Option<&Path>,
    json_out: Option<&Path>,
    resize: bool,
    bbox_threshold: u8,
    bbox_min_area: u32,
    bbox_pad: u32,
    max_boxes: usize,
    annotated_out: Option<&Path>,
    annotate_spec_out: Option<&Path>,
) -> Result<DiffRunOutput> {
    if !baseline_path.exists() {
        bail!("baseline not found: {}", baseline_path.display());
    }
    if !current_path.exists() {
        bail!("current not found: {}", current_path.display());
    }

    let baseline_image = image::open(baseline_path)
        .with_context(|| format!("failed to open baseline image: {}", baseline_path.display()))?;
    let mut current_image = image::open(current_path)
        .with_context(|| format!("failed to open current image: {}", current_path.display()))?;

    let mut resized = false;
    if baseline_image.dimensions() != current_image.dimensions() {
        if resize {
            let (w, h) = baseline_image.dimensions();
            current_image = current_image.resize_exact(w, h, FilterType::Lanczos3);
            resized = true;
        } else {
            bail!("image sizes differ. Re-run with --resize to match baseline size.");
        }
    }

    let baseline_rgba = baseline_image.to_rgba8();
    let current_rgba = current_image.to_rgba8();
    let (width, height) = baseline_rgba.dimensions();

    let total_pixels = (width as u64) * (height as u64);
    let mut changed_pixels: u64 = 0;
    let mut diff_sum: u64 = 0;
    let mut gray = vec![0u8; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let a = baseline_rgba.get_pixel(x, y).channels();
            let b = current_rgba.get_pixel(x, y).channels();

            let dr = (a[0] as i16 - b[0] as i16).unsigned_abs() as u8;
            let dg = (a[1] as i16 - b[1] as i16).unsigned_abs() as u8;
            let db = (a[2] as i16 - b[2] as i16).unsigned_abs() as u8;
            let diff_v = dr.max(dg).max(db);
            gray[idx] = diff_v;
            diff_sum += diff_v as u64;
            if diff_v > 0 {
                changed_pixels += 1;
            }
        }
    }

    let regions = extract_change_regions(
        &gray,
        width,
        height,
        bbox_threshold,
        bbox_min_area,
        bbox_pad,
        max_boxes,
    );

    if let Some(path) = diff_out {
        write_diff_overlay(&current_rgba, &gray, width, height, path)?;
    }

    let annotate_spec = build_annotate_spec(&regions);

    if let Some(path) = annotate_spec_out {
        write_json_pretty(path, &annotate_spec)?;
    }

    if let Some(path) = annotated_out {
        let mut annotated = current_rgba.clone();
        for region in &regions {
            draw_rect_outline(
                &mut annotated,
                region.x,
                region.y,
                region.w,
                region.h,
                Rgba([255, 69, 58, 255]),
                3,
            );
        }
        ensure_parent_dir(path)?;
        DynamicImage::ImageRgba8(annotated)
            .save(path)
            .with_context(|| format!("failed to save annotated image: {}", path.display()))?;
    }

    let percent_changed = if total_pixels > 0 {
        (changed_pixels as f64 / total_pixels as f64) * 100.0
    } else {
        0.0
    };
    let avg_diff_percent = if total_pixels > 0 {
        (diff_sum as f64 / (255.0 * total_pixels as f64)) * 100.0
    } else {
        0.0
    };

    let result = json!({
        "baseline": abs_path(baseline_path).display().to_string(),
        "current": abs_path(current_path).display().to_string(),
        "diff_image": diff_out.map(|p| abs_path(p).display().to_string()),
        "annotated_image": annotated_out.map(|p| abs_path(p).display().to_string()),
        "annotate_spec": annotate_spec_out.map(|p| abs_path(p).display().to_string()),
        "percent_changed": round_to(percent_changed, 3),
        "avg_diff_percent": round_to(avg_diff_percent, 3),
        "size": {"width": width, "height": height},
        "resized": resized,
        "change_regions": regions,
        "change_region_count": regions.len(),
    });

    if let Some(path) = json_out {
        write_json_pretty(path, &result)?;
    }

    Ok(DiffRunOutput { json: result })
}

fn extract_change_regions(
    gray: &[u8],
    width: u32,
    height: u32,
    threshold: u8,
    min_pixels: u32,
    pad: u32,
    max_boxes: usize,
) -> Vec<ChangeRegion> {
    let total = (width * height) as usize;
    let mut active = vec![false; total];
    let mut visited = vec![false; total];

    for (idx, val) in gray.iter().enumerate() {
        if *val > threshold {
            active[idx] = true;
        }
    }

    let mut raw_regions: Vec<(u32, u32, u32, u32, u32)> = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let start = (y * width + x) as usize;
            if visited[start] || !active[start] {
                continue;
            }

            let mut queue = std::collections::VecDeque::new();
            queue.push_back(start);
            visited[start] = true;

            let mut minx = x;
            let mut maxx = x;
            let mut miny = y;
            let mut maxy = y;
            let mut count: u32 = 0;

            while let Some(node) = queue.pop_front() {
                let cx = (node as u32) % width;
                let cy = (node as u32) / width;
                count += 1;

                if cx < minx {
                    minx = cx;
                }
                if cx > maxx {
                    maxx = cx;
                }
                if cy < miny {
                    miny = cy;
                }
                if cy > maxy {
                    maxy = cy;
                }

                if cx > 0 {
                    let left = node - 1;
                    if active[left] && !visited[left] {
                        visited[left] = true;
                        queue.push_back(left);
                    }
                }
                if cx + 1 < width {
                    let right = node + 1;
                    if active[right] && !visited[right] {
                        visited[right] = true;
                        queue.push_back(right);
                    }
                }
                if cy > 0 {
                    let up = node - width as usize;
                    if active[up] && !visited[up] {
                        visited[up] = true;
                        queue.push_back(up);
                    }
                }
                if cy + 1 < height {
                    let down = node + width as usize;
                    if active[down] && !visited[down] {
                        visited[down] = true;
                        queue.push_back(down);
                    }
                }
            }

            if count < min_pixels.max(1) {
                continue;
            }

            raw_regions.push((minx, miny, maxx, maxy, count));
        }
    }

    raw_regions.sort_by(|a, b| b.4.cmp(&a.4));
    if max_boxes > 0 && raw_regions.len() > max_boxes {
        raw_regions.truncate(max_boxes);
    }

    let mut regions = Vec::new();

    for (idx, (minx, miny, maxx, maxy, pixels)) in raw_regions.into_iter().enumerate() {
        let x0 = minx.saturating_sub(pad);
        let y0 = miny.saturating_sub(pad);
        let x1 = (maxx + pad).min(width.saturating_sub(1));
        let y1 = (maxy + pad).min(height.saturating_sub(1));

        let box_w = (x1.saturating_sub(x0)) + 1;
        let box_h = (y1.saturating_sub(y0)) + 1;
        let area = box_w.saturating_mul(box_h);

        let coverage = if area > 0 {
            round_to(pixels as f64 / area as f64, 4)
        } else {
            0.0
        };

        regions.push(ChangeRegion {
            x: x0,
            y: y0,
            w: box_w,
            h: box_h,
            x2: x0 + box_w,
            y2: y0 + box_h,
            pixels,
            area,
            coverage,
            intent: "changed-region".to_string(),
            action: "inspect".to_string(),
            id: format!("change-{}", idx + 1),
            rel: RegionRel {
                x: if width > 0 {
                    round_to(x0 as f64 / width as f64, 6)
                } else {
                    0.0
                },
                y: if height > 0 {
                    round_to(y0 as f64 / height as f64, 6)
                } else {
                    0.0
                },
                w: if width > 0 {
                    round_to(box_w as f64 / width as f64, 6)
                } else {
                    0.0
                },
                h: if height > 0 {
                    round_to(box_h as f64 / height as f64, 6)
                } else {
                    0.0
                },
            },
        });
    }

    regions
}

fn build_annotate_spec(regions: &[ChangeRegion]) -> Value {
    let mut annotations = Vec::new();

    for (idx, region) in regions.iter().enumerate() {
        let rect_id = region.id.clone();
        annotations.push(json!({
            "type": "rect",
            "id": rect_id,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "color": "#FF453A",
            "width": 3,
            "intent": "changed-region",
            "action": "inspect",
        }));
        annotations.push(json!({
            "type": "text",
            "text": format!("Δ{}", idx + 1),
            "anchor": region.id,
            "anchor_pos": "top_left",
            "anchor_offset": [0, -18],
            "color": "#FFFFFF",
            "text_bg": "rgba(255,69,58,0.78)",
            "intent": "change-label",
            "action": "review-diff",
        }));
    }

    json!({
        "defaults": {
            "auto_scale": true,
            "outline": true,
            "text_bg": "rgba(0,0,0,0.6)",
        },
        "annotations": annotations,
    })
}

fn write_diff_overlay(
    current: &RgbaImage,
    gray: &[u8],
    width: u32,
    height: u32,
    out_path: &Path,
) -> Result<()> {
    let mut out = current.clone();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let alpha = gray[idx] as f32 / 255.0;
            if alpha <= 0.0 {
                continue;
            }
            let base = out.get_pixel(x, y).0;
            let red = [255f32, 0f32, 0f32];
            let blended = [
                ((1.0 - alpha) * base[0] as f32 + alpha * red[0]).round() as u8,
                ((1.0 - alpha) * base[1] as f32 + alpha * red[1]).round() as u8,
                ((1.0 - alpha) * base[2] as f32 + alpha * red[2]).round() as u8,
                base[3],
            ];
            out.put_pixel(x, y, Rgba(blended));
        }
    }

    ensure_parent_dir(out_path)?;
    DynamicImage::ImageRgba8(out)
        .save(out_path)
        .with_context(|| format!("failed to save diff image: {}", out_path.display()))?;
    Ok(())
}

fn draw_rect_outline(
    img: &mut RgbaImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: Rgba<u8>,
    thickness: u32,
) {
    if w == 0 || h == 0 {
        return;
    }
    let img_w = img.width();
    let img_h = img.height();

    let x0 = x.min(img_w.saturating_sub(1));
    let y0 = y.min(img_h.saturating_sub(1));
    let x1 = (x + w.saturating_sub(1)).min(img_w.saturating_sub(1));
    let y1 = (y + h.saturating_sub(1)).min(img_h.saturating_sub(1));

    for t in 0..thickness.max(1) {
        let tx0 = x0.saturating_sub(t);
        let ty0 = y0.saturating_sub(t);
        let tx1 = (x1 + t).min(img_w.saturating_sub(1));
        let ty1 = (y1 + t).min(img_h.saturating_sub(1));

        for xx in tx0..=tx1 {
            img.put_pixel(xx, ty0, color);
            img.put_pixel(xx, ty1, color);
        }
        for yy in ty0..=ty1 {
            img.put_pixel(tx0, yy, color);
            img.put_pixel(tx1, yy, color);
        }
    }
}

fn capture_internal(
    out_path: &Path,
    process: Option<String>,
    step: Option<&str>,
    note: Option<&str>,
    sidecar: Option<&Path>,
) -> Result<Value> {
    ensure_parent_dir(out_path)?;

    let process_name = process
        .clone()
        .or_else(frontmost_app_name)
        .unwrap_or_else(|| "app".to_string());
    let app_slug = slugify(&process_name);

    let mut x: i64 = 0;
    let mut y: i64 = 0;
    let mut w: i64 = 0;
    let mut h: i64 = 0;
    let mut window_title: Option<String> = None;
    let mut selected_window_index: Option<usize> = None;
    let mut candidate_count: usize = 0;
    let mut usable_count: usize = 0;
    let mut selection_mode = "none".to_string();
    let mut selected_window_usable = false;
    let mut usable_min_w: i64 = 0;
    let mut usable_min_h: i64 = 0;
    let mut usable_min_area: i64 = 0;

    let mut captured = false;
    let mut capture_mode = "fallback".to_string();
    let mut warnings: Vec<String> = Vec::new();
    let (query_window_diag, activation_diag) = if cfg!(target_os = "macos") {
        let activation_diag = activate_process_window(&process_name);
        let probe = query_window_probe(&process_name);
        let query_window_diag = probe.diagnostics.clone();
        if probe.diagnostics.ok {
            x = probe.x;
            y = probe.y;
            w = probe.w;
            h = probe.h;
            window_title = probe.title;
            selected_window_index = probe.selected_index;
            candidate_count = probe.candidate_count;
            usable_count = probe.usable_count;
            selection_mode = probe.selection_mode.clone();
            selected_window_usable = probe.usable;
            usable_min_w = probe.min_width;
            usable_min_h = probe.min_height;
            usable_min_area = probe.min_area;
        }

        if !query_window_diag.ok {
            if let Some(code) = query_window_diag.error_code.as_deref() {
                warnings.push(format!("window_query:{code}"));
            }
            if let Some(message) = query_window_diag.message.as_deref() {
                warnings.push(format!("window query unavailable: {message}"));
            }
        }
        if !activation_diag.ok {
            if let Some(code) = activation_diag.error_code.as_deref() {
                warnings.push(format!("activate:{code}"));
            }
            if let Some(message) = activation_diag.message.as_deref() {
                warnings.push(format!("activation note: {message}"));
            }
        }

        if query_window_diag.ok && w > 0 && h > 0 && command_exists("screencapture") {
            if selected_window_usable {
                let region = format!("{x},{y},{w},{h}");
                if Command::new("screencapture")
                    .arg("-x")
                    .arg("-R")
                    .arg(region)
                    .arg(out_path)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                {
                    captured = true;
                    capture_mode = "window".to_string();
                } else {
                    warnings
                        .push("window capture failed; attempting full-screen fallback".to_string());
                }
            } else {
                warnings.push(format!(
                    "selected window {}x{} is below usable threshold {}x{} / area {}; using full-screen fallback",
                    w, h, usable_min_w, usable_min_h, usable_min_area
                ));
            }
        }

        if !captured && command_exists("screencapture") {
            captured = Command::new("screencapture")
                .arg("-x")
                .arg(out_path)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if captured {
                capture_mode = "screen".to_string();
                warnings.push(
                    "window-bounds capture unavailable; used full-screen capture fallback"
                        .to_string(),
                );
            }
        }
        (query_window_diag, activation_diag)
    } else {
        warnings.push("window capture uses placeholder on non-macOS hosts".to_string());
        let query_window_diag = QueryDiagnostic {
            ok: false,
            attempts: 0,
            error_code: Some("unsupported_platform".to_string()),
            message: Some("window queries require macOS System Events".to_string()),
        };
        let activation_diag = query_window_diag.clone();
        (query_window_diag, activation_diag)
    };

    if !captured {
        let fallback = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            1280,
            720,
            Rgba([255, 255, 255, 255]),
        ));
        fallback
            .save(out_path)
            .with_context(|| format!("failed to write fallback capture: {}", out_path.display()))?;
        if w == 0 || h == 0 {
            w = 1280;
            h = 720;
        }
        if window_title.is_none() {
            window_title = Some("fallback-window".to_string());
        }
        warnings.push("capture failed; generated placeholder image".to_string());
    }

    let output_img = image::open(out_path)
        .with_context(|| format!("failed to read capture image: {}", out_path.display()))?;
    let (image_w, image_h) = output_img.dimensions();

    let scale_x = if w > 0 {
        Some(round_to(image_w as f64 / w as f64, 6))
    } else {
        None
    };
    let scale_y = if h > 0 {
        Some(round_to(image_h as f64 / h as f64, 6))
    } else {
        None
    };
    let scale = match (scale_x, scale_y) {
        (Some(sx), Some(sy)) => Some(round_to((sx + sy) / 2.0, 6)),
        _ => None,
    };

    let sidecar_abs = sidecar.map(abs_path);

    let payload = json!({
        "image_path": abs_path(out_path).display().to_string(),
        "capture_path": abs_path(out_path).display().to_string(),
        "sidecar_path": sidecar_abs.as_ref().map(|p| p.display().to_string()),
        "captured_at": timestamp_iso(),
        "captured_at_epoch_ms": Utc::now().timestamp_millis(),
        "app_name": process_name,
        "app_slug": app_slug,
        "window_title": window_title,
        "step": step,
        "note": note,
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
        "scale": scale,
        "scale_x": scale_x,
        "scale_y": scale_y,
        "window": {
            "x": x,
            "y": y,
            "w": w,
            "h": h,
            "x2": x + w,
            "y2": y + h,
            "units": "px",
        },
        "capture_tool": "codex-visual-loop capture",
        "capture_sidecar_version": 1,
        "capture_mode": capture_mode,
        "fallback_used": !captured,
        "warnings": warnings,
        "window_probe": {
            "selected_index": selected_window_index,
            "selection_mode": selection_mode,
            "candidate_count": candidate_count,
            "usable_count": usable_count,
            "usable": selected_window_usable,
            "min_width": usable_min_w,
            "min_height": usable_min_h,
            "min_area": usable_min_area,
        },
        "query": {
            "activation": activation_diag,
            "window": query_window_diag,
        },
    });

    if let Some(path) = sidecar {
        write_json_pretty(path, &payload)?;
    }

    Ok(payload)
}

fn load_spec(path: &str) -> Result<Value> {
    let raw = if path == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .context("failed to read spec from stdin")?;
        buf
    } else {
        fs::read_to_string(path).with_context(|| format!("failed to read spec file: {path}"))?
    };

    let value: Value = serde_json::from_str(&raw).context("invalid spec JSON")?;
    match value {
        Value::Array(arr) => Ok(json!({"annotations": arr, "defaults": {}})),
        Value::Object(mut obj) => {
            if obj.get("annotations").is_none() {
                bail!("spec must be a list or an object with 'annotations'");
            }
            if obj.get("defaults").is_none() {
                obj.insert("defaults".to_string(), Value::Object(Map::new()));
            }
            Ok(Value::Object(obj))
        }
        _ => bail!("spec must be a list or an object with 'annotations'"),
    }
}

fn resolve_annotation_units(
    ann: &mut Map<String, Value>,
    img_w: u32,
    img_h: u32,
    defaults: &Map<String, Value>,
) {
    let units_value = ann.get("units").or_else(|| defaults.get("units")).cloned();
    let default_rel = units_is_rel(units_value.as_ref());

    let fields = [
        ("x", img_w as f64),
        ("x1", img_w as f64),
        ("x2", img_w as f64),
        ("w", img_w as f64),
        ("y", img_h as f64),
        ("y1", img_h as f64),
        ("y2", img_h as f64),
        ("h", img_h as f64),
    ];

    for (key, span) in fields {
        if let Some(value) = ann.get(key).cloned() {
            if let Some(resolved) = resolve_measure(&value, span, default_rel) {
                ann.insert(key.to_string(), json!(resolved));
            }
        }
    }

    for key in ["anchor_offset", "from_offset", "to_offset"] {
        if let Some(offset) = ann.get(key).cloned() {
            if let Some(resolved) = resolve_offset_units(&offset, img_w, img_h, default_rel) {
                ann.insert(key.to_string(), resolved);
            }
        }
    }

    for anchor_key in ["anchor", "from", "to"] {
        let Some(Value::Object(anchor_obj)) = ann.get(anchor_key).cloned() else {
            continue;
        };
        if !anchor_obj.contains_key("offset") {
            continue;
        }

        let mut updated = anchor_obj.clone();
        let anchor_units = updated
            .get("units")
            .or_else(|| ann.get("units"))
            .or_else(|| defaults.get("units"))
            .cloned();
        let anchor_rel = units_is_rel(anchor_units.as_ref());
        if let Some(offset) = updated.get("offset").cloned() {
            if let Some(resolved) = resolve_offset_units(&offset, img_w, img_h, anchor_rel) {
                updated.insert("offset".to_string(), resolved);
            }
        }
        ann.insert(anchor_key.to_string(), Value::Object(updated));
    }

    if let Some(Value::Object(fit)) = ann.get("fit").cloned() {
        let mut updated = fit.clone();
        let fit_units = updated
            .get("units")
            .or_else(|| ann.get("units"))
            .or_else(|| defaults.get("units"))
            .cloned();
        let fit_rel = units_is_rel(fit_units.as_ref());

        if let Some(region) = updated.get("region").cloned() {
            if let Some(resolved) = resolve_region_units(&region, img_w, img_h, fit_rel) {
                updated.insert("region".to_string(), resolved);
            }
        }
        if let Some(pad) = updated.get("pad").cloned() {
            if let Some(resolved) =
                resolve_measure(&pad, f64::from(img_w.max(img_h).max(1)), fit_rel)
            {
                updated.insert("pad".to_string(), json!(resolved));
            }
        }
        ann.insert("fit".to_string(), Value::Object(updated));
    }
}

fn resolve_offset_units(value: &Value, img_w: u32, img_h: u32, default_rel: bool) -> Option<Value> {
    match value {
        Value::Array(values) if values.len() >= 2 => {
            let dx = resolve_measure(&values[0], f64::from(img_w), default_rel)?;
            let dy = resolve_measure(&values[1], f64::from(img_h), default_rel)?;
            Some(json!([dx, dy]))
        }
        Value::String(raw) => {
            let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
            if parts.len() < 2 {
                return None;
            }
            let dx = resolve_measure(
                &Value::String(parts[0].to_string()),
                f64::from(img_w),
                default_rel,
            )?;
            let dy = resolve_measure(
                &Value::String(parts[1].to_string()),
                f64::from(img_h),
                default_rel,
            )?;
            Some(json!([dx, dy]))
        }
        _ => None,
    }
}

fn resolve_region_units(value: &Value, img_w: u32, img_h: u32, default_rel: bool) -> Option<Value> {
    match value {
        Value::Object(obj) => {
            let x = resolve_measure(obj.get("x")?, f64::from(img_w), default_rel)?;
            let y = resolve_measure(obj.get("y")?, f64::from(img_h), default_rel)?;
            let w = resolve_measure(obj.get("w")?, f64::from(img_w), default_rel)?;
            let h = resolve_measure(obj.get("h")?, f64::from(img_h), default_rel)?;
            Some(json!({"x": x, "y": y, "w": w, "h": h}))
        }
        Value::Array(values) if values.len() >= 4 => {
            let x = resolve_measure(&values[0], f64::from(img_w), default_rel)?;
            let y = resolve_measure(&values[1], f64::from(img_h), default_rel)?;
            let w = resolve_measure(&values[2], f64::from(img_w), default_rel)?;
            let h = resolve_measure(&values[3], f64::from(img_h), default_rel)?;
            Some(json!([x, y, w, h]))
        }
        _ => None,
    }
}

fn units_is_rel(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(v)) => *v,
        Some(Value::String(s)) => {
            let key = s.trim().to_ascii_lowercase();
            matches!(
                key.as_str(),
                "rel" | "relative" | "ratio" | "fraction" | "normalized"
            )
        }
        _ => false,
    }
}

fn resolve_measure(value: &Value, span: f64, default_rel: bool) -> Option<f64> {
    match value {
        Value::Number(n) => {
            let v = n.as_f64()?;
            if default_rel {
                Some(v * span)
            } else {
                Some(v)
            }
        }
        Value::String(s) => {
            let raw = s.trim().to_ascii_lowercase();
            if raw.is_empty() {
                return None;
            }

            if let Some(percent) = raw.strip_suffix('%') {
                return percent.parse::<f64>().ok().map(|v| v * span / 100.0);
            }

            if let Some(rel) = raw.strip_suffix("rel") {
                if let Ok(mut ratio) = rel.parse::<f64>() {
                    if ratio.abs() > 1.0 {
                        ratio /= 100.0;
                    }
                    return Some(ratio * span);
                }
            }

            if let Some(px) = raw.strip_suffix("px") {
                return px.parse::<f64>().ok();
            }

            if let Ok(v) = raw.parse::<f64>() {
                if default_rel {
                    return Some(v * span);
                }
                return Some(v);
            }

            None
        }
        _ => None,
    }
}

fn annotation_meta_item(index: usize, ann: &Map<String, Value>, img_w: u32, img_h: u32) -> Value {
    let ann_type = ann
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    let mut item = Map::new();
    item.insert("index".to_string(), json!(index));
    item.insert("type".to_string(), json!(ann_type));

    for key in [
        "id",
        "units",
        "intent",
        "action",
        "severity",
        "issue",
        "hypothesis",
        "next_action",
        "verify",
    ] {
        item.insert(
            key.to_string(),
            ann.get(key).cloned().unwrap_or(Value::Null),
        );
    }

    let geometry = extract_geometry(ann, &ann_type);
    if !geometry.is_empty() {
        let geometry_val = Value::Object(geometry.clone());
        item.insert("geometry".to_string(), geometry_val);
        let rel = geometry_rel(&geometry, &ann_type, img_w, img_h);
        if !rel.is_empty() {
            item.insert("geometry_rel".to_string(), Value::Object(rel));
        }
    }

    if let Some(text) = ann.get("text").and_then(Value::as_str) {
        if !text.is_empty() {
            item.insert("text".to_string(), json!(text));
        }
    }

    Value::Object(item)
}

fn extract_geometry(ann: &Map<String, Value>, ann_type: &str) -> Map<String, Value> {
    let keys: &[&str] = match ann_type {
        "rect" | "spotlight" | "focus" | "dim" => &["x", "y", "w", "h"],
        "arrow" => &["x1", "y1", "x2", "y2"],
        "text" => &["x", "y"],
        _ => &[],
    };

    let mut geometry = Map::new();
    for key in keys {
        if let Some(value) = ann.get(*key) {
            if let Some(n) = normalize_number(value) {
                geometry.insert((*key).to_string(), n);
            }
        }
    }
    geometry
}

fn geometry_rel(
    geometry: &Map<String, Value>,
    ann_type: &str,
    img_w: u32,
    img_h: u32,
) -> Map<String, Value> {
    let mut rel = Map::new();

    for (key, value) in geometry {
        let num = match value.as_f64() {
            Some(v) => v,
            None => continue,
        };

        if matches!(key.as_str(), "x" | "x1" | "x2" | "w") && img_w > 0 {
            rel.insert(key.clone(), json!(round_to(num / img_w as f64, 6)));
        } else if matches!(key.as_str(), "y" | "y1" | "y2" | "h") && img_h > 0 {
            rel.insert(key.clone(), json!(round_to(num / img_h as f64, 6)));
        }
    }

    if matches!(ann_type, "rect" | "spotlight" | "focus" | "dim") {
        let x = geometry.get("x").and_then(Value::as_f64);
        let y = geometry.get("y").and_then(Value::as_f64);
        let w = geometry.get("w").and_then(Value::as_f64);
        let h = geometry.get("h").and_then(Value::as_f64);

        if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
            rel.insert(
                "bbox".to_string(),
                json!({
                    "x": if img_w > 0 { round_to(x / img_w as f64, 6) } else { 0.0 },
                    "y": if img_h > 0 { round_to(y / img_h as f64, 6) } else { 0.0 },
                    "w": if img_w > 0 { round_to(w / img_w as f64, 6) } else { 0.0 },
                    "h": if img_h > 0 { round_to(h / img_h as f64, 6) } else { 0.0 },
                }),
            );
        }
    }

    rel
}

fn normalize_number(value: &Value) -> Option<Value> {
    if value.is_boolean() {
        return Some(value.clone());
    }
    if let Some(i) = value.as_i64() {
        return Some(json!(i));
    }
    if let Some(f) = value.as_f64() {
        let rounded = round_to(f, 4);
        if (rounded - rounded.round()).abs() < 1e-6 {
            return Some(json!(rounded.round() as i64));
        }
        return Some(json!(rounded));
    }
    None
}

fn annotation_type(ann: &Map<String, Value>) -> String {
    ann.get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn is_spotlight_type(kind: &str) -> bool {
    matches!(kind, "spotlight" | "focus" | "dim")
}

fn resolve_scale(defaults: &Map<String, Value>, img_w: u32, img_h: u32) -> f64 {
    if let Some(scale) = defaults.get("scale").and_then(Value::as_f64) {
        return scale.max(0.1);
    }

    let mut auto_scale = true;
    if let Some(value) = defaults.get("auto_scale") {
        auto_scale = value_to_bool(value, true);
    }
    if !auto_scale {
        return 1.0;
    }
    let max_dim = f64::from(img_w.max(img_h).max(1));
    let raw = max_dim / 1200.0;
    raw.clamp(1.0, 2.0)
}

fn value_to_bool(value: &Value, default_value: bool) -> bool {
    match value {
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_i64().map(|v| v != 0).unwrap_or(default_value),
        Value::String(s) => {
            let key = s.trim().to_ascii_lowercase();
            if matches!(key.as_str(), "1" | "true" | "yes" | "on") {
                true
            } else if matches!(key.as_str(), "0" | "false" | "no" | "off") {
                false
            } else {
                default_value
            }
        }
        _ => default_value,
    }
}

fn value_to_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn value_to_i32(value: Option<&Value>, fallback: i32) -> i32 {
    value_to_f64(value)
        .map(|v| v.round() as i32)
        .unwrap_or(fallback)
}

fn value_to_usize(value: Option<&Value>) -> Option<usize> {
    match value {
        Some(Value::Number(n)) => n.as_u64().map(|v| v as usize),
        Some(Value::String(s)) => s.trim().parse::<usize>().ok(),
        _ => None,
    }
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => {
            let v = s.trim();
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        }
        Some(v) if !v.is_null() => Some(v.to_string()),
        _ => None,
    }
}

fn parse_offset_value(value: Option<&Value>) -> Option<(f64, f64)> {
    let value = value?;
    match value {
        Value::Array(items) if items.len() >= 2 => {
            Some((value_to_f64(items.first())?, value_to_f64(items.get(1))?))
        }
        Value::String(raw) => {
            let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
            if parts.len() < 2 {
                return None;
            }
            let x = parts[0].parse::<f64>().ok()?;
            let y = parts[1].parse::<f64>().ok()?;
            Some((x, y))
        }
        _ => None,
    }
}

fn parse_color(value: Option<&Value>, fallback: [u8; 4]) -> Rgba<u8> {
    parse_color_opt(value).unwrap_or(Rgba(fallback))
}

fn parse_color_opt(value: Option<&Value>) -> Option<Rgba<u8>> {
    let raw = value_to_string(value)?;
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Rgba([r, g, b, 255]));
        }
        if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            return Some(Rgba([r, g, b, a]));
        }
    }

    let lower = s.to_ascii_lowercase();
    if lower.starts_with("rgba(") && lower.ends_with(')') {
        let body = &lower[5..lower.len() - 1];
        let parts: Vec<&str> = body.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return None;
        }
        let r = parts[0].parse::<f64>().ok()?.round().clamp(0.0, 255.0) as u8;
        let g = parts[1].parse::<f64>().ok()?.round().clamp(0.0, 255.0) as u8;
        let b = parts[2].parse::<f64>().ok()?.round().clamp(0.0, 255.0) as u8;
        let alpha_value = parts[3].parse::<f64>().ok()?;
        let a = if alpha_value <= 1.0 {
            (alpha_value * 255.0).round().clamp(0.0, 255.0) as u8
        } else {
            alpha_value.round().clamp(0.0, 255.0) as u8
        };
        return Some(Rgba([r, g, b, a]));
    }

    None
}

fn color_luma(color: Rgba<u8>) -> f64 {
    let [r, g, b, _] = color.0;
    (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)) / 255.0
}

fn auto_outline_color(color: Rgba<u8>) -> Rgba<u8> {
    if color_luma(color) > 0.6 {
        Rgba([0, 0, 0, 220])
    } else {
        Rgba([255, 255, 255, 220])
    }
}

fn scale_default(value: f64, scale: f64, min_value: u32) -> u32 {
    ((value * scale).round() as i64).max(i64::from(min_value)) as u32
}

fn clamp_i32(value: i32, min_value: i32, max_value: i32) -> i32 {
    value.max(min_value).min(max_value)
}

fn blend_pixel(dst: Rgba<u8>, src: Rgba<u8>) -> Rgba<u8> {
    let a = f64::from(src[3]) / 255.0;
    if a <= 0.0 {
        return dst;
    }
    let inv = 1.0 - a;
    let r = (f64::from(dst[0]) * inv + f64::from(src[0]) * a)
        .round()
        .clamp(0.0, 255.0) as u8;
    let g = (f64::from(dst[1]) * inv + f64::from(src[1]) * a)
        .round()
        .clamp(0.0, 255.0) as u8;
    let b = (f64::from(dst[2]) * inv + f64::from(src[2]) * a)
        .round()
        .clamp(0.0, 255.0) as u8;
    let out_a = (f64::from(dst[3]) + f64::from(src[3]) * inv)
        .round()
        .clamp(0.0, 255.0) as u8;
    Rgba([r, g, b, out_a])
}

fn draw_disc(img: &mut RgbaImage, cx: f64, cy: f64, radius: f64, color: Rgba<u8>) {
    if radius <= 0.1 {
        let x = cx.round() as i32;
        let y = cy.round() as i32;
        if x >= 0 && y >= 0 && x < img.width() as i32 && y < img.height() as i32 {
            let dst = *img.get_pixel(x as u32, y as u32);
            img.put_pixel(x as u32, y as u32, blend_pixel(dst, color));
        }
        return;
    }
    let min_x = clamp_i32((cx - radius).floor() as i32, 0, img.width() as i32 - 1);
    let max_x = clamp_i32((cx + radius).ceil() as i32, 0, img.width() as i32 - 1);
    let min_y = clamp_i32((cy - radius).floor() as i32, 0, img.height() as i32 - 1);
    let max_y = clamp_i32((cy + radius).ceil() as i32, 0, img.height() as i32 - 1);
    let r2 = radius * radius;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = f64::from(x) - cx;
            let dy = f64::from(y) - cy;
            if dx * dx + dy * dy <= r2 {
                let dst = *img.get_pixel(x as u32, y as u32);
                img.put_pixel(x as u32, y as u32, blend_pixel(dst, color));
            }
        }
    }
}

fn draw_thick_line(
    img: &mut RgbaImage,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    color: Rgba<u8>,
    width: f64,
) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let distance = (dx * dx + dy * dy).sqrt();
    let steps = distance.max(1.0).ceil() as i32;
    let radius = (width.max(1.0) / 2.0).max(0.6);
    for step in 0..=steps {
        let t = f64::from(step) / f64::from(steps.max(1));
        let x = x1 + dx * t;
        let y = y1 + dy * t;
        draw_disc(img, x, y, radius, color);
    }
}

fn triangle_area(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    ((a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1)).abs()) / 2.0
}

fn point_in_triangle(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64), eps: f64) -> bool {
    let total = triangle_area(a, b, c);
    if total <= eps {
        return false;
    }
    let a1 = triangle_area(p, b, c);
    let a2 = triangle_area(a, p, c);
    let a3 = triangle_area(a, b, p);
    (a1 + a2 + a3 - total).abs() <= eps
}

fn fill_triangle(
    img: &mut RgbaImage,
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    color: Rgba<u8>,
) {
    let min_x = clamp_i32(
        a.0.min(b.0).min(c.0).floor() as i32,
        0,
        img.width() as i32 - 1,
    );
    let max_x = clamp_i32(
        a.0.max(b.0).max(c.0).ceil() as i32,
        0,
        img.width() as i32 - 1,
    );
    let min_y = clamp_i32(
        a.1.min(b.1).min(c.1).floor() as i32,
        0,
        img.height() as i32 - 1,
    );
    let max_y = clamp_i32(
        a.1.max(b.1).max(c.1).ceil() as i32,
        0,
        img.height() as i32 - 1,
    );
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            if point_in_triangle(p, a, b, c, 0.8) {
                let dst = *img.get_pixel(x as u32, y as u32);
                img.put_pixel(x as u32, y as u32, blend_pixel(dst, color));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_arrow_primitive(
    img: &mut RgbaImage,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    color: Rgba<u8>,
    width: f64,
    head_len: f64,
    head_width: f64,
) {
    let angle = (y2 - y1).atan2(x2 - x1);
    let back_x = x2 - head_len * angle.cos();
    let back_y = y2 - head_len * angle.sin();
    draw_thick_line(img, x1, y1, back_x, back_y, color, width);

    let left_angle = angle + PI / 2.0;
    let right_angle = angle - PI / 2.0;
    let left = (
        back_x + (head_width / 2.0) * left_angle.cos(),
        back_y + (head_width / 2.0) * left_angle.sin(),
    );
    let right = (
        back_x + (head_width / 2.0) * right_angle.cos(),
        back_y + (head_width / 2.0) * right_angle.sin(),
    );
    fill_triangle(img, (x2, y2), left, right, color);
}

fn draw_bitmap_text(img: &mut RgbaImage, x: i32, y: i32, text: &str, color: Rgba<u8>, scale: u32) {
    let scale_i = scale.max(1) as i32;
    let mut cursor_x = x;
    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = x;
            continue;
        }
        let glyph = BASIC_FONTS.get(ch).or_else(|| BASIC_FONTS.get('?'));
        let Some(glyph) = glyph else {
            cursor_x += 8 * scale_i;
            continue;
        };
        for (row_idx, row) in glyph.iter().enumerate() {
            let row_bits = *row;
            for col_idx in 0..8 {
                if (row_bits >> col_idx) & 1 == 0 {
                    continue;
                }
                let px = cursor_x + col_idx * scale_i;
                let py = y + row_idx as i32 * scale_i;
                for sy in 0..scale_i {
                    for sx in 0..scale_i {
                        let tx = px + sx;
                        let ty = py + sy;
                        if tx >= 0 && ty >= 0 && tx < img.width() as i32 && ty < img.height() as i32
                        {
                            let dst = *img.get_pixel(tx as u32, ty as u32);
                            img.put_pixel(tx as u32, ty as u32, blend_pixel(dst, color));
                        }
                    }
                }
            }
        }
        cursor_x += 8 * scale_i;
    }
}

fn text_bbox(x: i32, y: i32, text: &str, scale: u32) -> (i32, i32, i32, i32) {
    let scale_i = scale.max(1) as i32;
    let lines: Vec<&str> = text.split('\n').collect();
    let width_chars = lines
        .iter()
        .map(|line| line.chars().count() as i32)
        .max()
        .unwrap_or(0);
    let line_count = lines.len().max(1) as i32;
    (
        x,
        y,
        x + width_chars * 8 * scale_i,
        y + line_count * 8 * scale_i,
    )
}

fn fill_rect_alpha(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
    if img.width() == 0 || img.height() == 0 {
        return;
    }
    let min_x = clamp_i32(x0.min(x1), 0, img.width() as i32 - 1);
    let max_x = clamp_i32(x0.max(x1), 0, img.width() as i32 - 1);
    let min_y = clamp_i32(y0.min(y1), 0, img.height() as i32 - 1);
    let max_y = clamp_i32(y0.max(y1), 0, img.height() as i32 - 1);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dst = *img.get_pixel(x as u32, y as u32);
            img.put_pixel(x as u32, y as u32, blend_pixel(dst, color));
        }
    }
}

fn bbox_from_ann(ann: &Map<String, Value>) -> Option<(f64, f64, f64, f64)> {
    let x = value_to_f64(ann.get("x"))?;
    let y = value_to_f64(ann.get("y"))?;
    let w = value_to_f64(ann.get("w"))?;
    let h = value_to_f64(ann.get("h"))?;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some((x, y, x + w, y + h))
}

fn anchor_point(bbox: (f64, f64, f64, f64), pos: &str) -> (f64, f64) {
    let (x0, y0, x1, y1) = bbox;
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    match pos.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "top" => (cx, y0),
        "bottom" => (cx, y1),
        "left" => (x0, cy),
        "right" => (x1, cy),
        "top_left" => (x0, y0),
        "top_right" => (x1, y0),
        "bottom_left" => (x0, y1),
        "bottom_right" => (x1, y1),
        _ => (cx, cy),
    }
}

fn anchor_target_center(target: &AnchorTarget) -> (f64, f64) {
    (
        (target.bbox.0 + target.bbox.2) / 2.0,
        (target.bbox.1 + target.bbox.3) / 2.0,
    )
}

fn normalize_anchor_spec(value: Option<&Value>) -> Option<AnchorSpec> {
    let value = value?;
    match value {
        Value::Bool(v) => {
            if *v {
                Some(AnchorSpec {
                    id: None,
                    index: None,
                    nearest: true,
                    target_type: None,
                    pos: None,
                    offset: None,
                })
            } else {
                None
            }
        }
        Value::Number(n) => Some(AnchorSpec {
            id: None,
            index: n.as_u64().map(|v| v as usize),
            nearest: false,
            target_type: None,
            pos: None,
            offset: None,
        }),
        Value::String(s) => {
            let raw = s.trim();
            if raw.is_empty() {
                return None;
            }
            if raw.eq_ignore_ascii_case("nearest") {
                Some(AnchorSpec {
                    id: None,
                    index: None,
                    nearest: true,
                    target_type: None,
                    pos: None,
                    offset: None,
                })
            } else {
                Some(AnchorSpec {
                    id: Some(raw.to_string()),
                    index: None,
                    nearest: false,
                    target_type: None,
                    pos: None,
                    offset: None,
                })
            }
        }
        Value::Object(obj) => Some(AnchorSpec {
            id: value_to_string(obj.get("id")),
            index: value_to_usize(obj.get("index")),
            nearest: obj
                .get("nearest")
                .map(|v| value_to_bool(v, false))
                .unwrap_or(false),
            target_type: value_to_string(obj.get("type")),
            pos: value_to_string(obj.get("pos")),
            offset: parse_offset_value(obj.get("offset")),
        }),
        _ => None,
    }
}

fn resolve_target<'a>(
    spec: &AnchorSpec,
    targets: &'a [AnchorTarget],
    fallback_point: (f64, f64),
) -> Option<&'a AnchorTarget> {
    if targets.is_empty() {
        return None;
    }

    let mut candidates: Vec<&AnchorTarget> = targets.iter().collect();
    if let Some(kind) = &spec.target_type {
        candidates.retain(|target| target.ann_type.eq_ignore_ascii_case(kind));
    }
    if candidates.is_empty() {
        return None;
    }

    if let Some(id) = &spec.id {
        if let Some(found) = candidates
            .iter()
            .copied()
            .find(|target| target.id.as_deref() == Some(id.as_str()))
        {
            return Some(found);
        }
    }

    if let Some(index) = spec.index {
        if let Some(found) = candidates
            .iter()
            .copied()
            .find(|target| target.index == index)
        {
            return Some(found);
        }
    }

    if spec.nearest || (spec.id.is_none() && spec.index.is_none() && spec.target_type.is_none()) {
        let mut best: Option<&AnchorTarget> = None;
        let mut best_dist = f64::MAX;
        for target in candidates {
            let center = anchor_target_center(target);
            let dx = center.0 - fallback_point.0;
            let dy = center.1 - fallback_point.1;
            let dist = dx * dx + dy * dy;
            if dist < best_dist {
                best = Some(target);
                best_dist = dist;
            }
        }
        return best;
    }

    None
}

fn resolve_anchor_pos(
    spec_pos: Option<String>,
    ann_pos: Option<String>,
    defaults: &Map<String, Value>,
    fallback: &str,
) -> String {
    spec_pos
        .or(ann_pos)
        .or_else(|| value_to_string(defaults.get("anchor_pos")))
        .unwrap_or_else(|| fallback.to_string())
}

fn resolve_anchor_offset(
    spec_offset: Option<(f64, f64)>,
    ann_offset: Option<(f64, f64)>,
    defaults: &Map<String, Value>,
    fallback: (f64, f64),
) -> (f64, f64) {
    spec_offset
        .or(ann_offset)
        .or_else(|| parse_offset_value(defaults.get("anchor_offset")))
        .unwrap_or(fallback)
}

fn apply_text_anchor(
    ann: &Map<String, Value>,
    targets: &[AnchorTarget],
    defaults: &Map<String, Value>,
    img_w: u32,
    img_h: u32,
) -> Map<String, Value> {
    let mut updated = ann.clone();
    let spec = normalize_anchor_spec(ann.get("anchor"));
    let Some(spec) = spec else {
        return updated;
    };
    let x = value_to_f64(ann.get("x")).unwrap_or(f64::from(img_w) / 2.0);
    let y = value_to_f64(ann.get("y")).unwrap_or(f64::from(img_h) / 2.0);
    let Some(target) = resolve_target(&spec, targets, (x, y)) else {
        return updated;
    };

    let pos = resolve_anchor_pos(
        spec.pos.clone(),
        value_to_string(ann.get("anchor_pos")),
        defaults,
        "top",
    );
    let offset = resolve_anchor_offset(
        spec.offset,
        parse_offset_value(ann.get("anchor_offset")),
        defaults,
        (0.0, 0.0),
    );
    let anchor = anchor_point(target.bbox, &pos);
    updated.insert("x".to_string(), json!(anchor.0 + offset.0));
    updated.insert("y".to_string(), json!(anchor.1 + offset.1));
    updated
}

fn apply_arrow_anchor(
    ann: &Map<String, Value>,
    targets: &[AnchorTarget],
    defaults: &Map<String, Value>,
    img_w: u32,
    img_h: u32,
) -> Map<String, Value> {
    let from_spec = normalize_anchor_spec(ann.get("from"));
    let to_spec = normalize_anchor_spec(ann.get("to"));
    if from_spec.is_none() && to_spec.is_none() {
        return ann.clone();
    }
    let mut updated = ann.clone();

    if let Some(spec) = from_spec {
        let x1 = value_to_f64(ann.get("x1")).unwrap_or(f64::from(img_w) / 2.0);
        let y1 = value_to_f64(ann.get("y1")).unwrap_or(f64::from(img_h) / 2.0);
        if let Some(target) = resolve_target(&spec, targets, (x1, y1)) {
            let pos = resolve_anchor_pos(
                spec.pos.clone(),
                value_to_string(ann.get("from_pos")),
                defaults,
                "center",
            );
            let offset = resolve_anchor_offset(
                spec.offset,
                parse_offset_value(ann.get("from_offset")),
                defaults,
                (0.0, 0.0),
            );
            let anchor = anchor_point(target.bbox, &pos);
            updated.insert("x1".to_string(), json!(anchor.0 + offset.0));
            updated.insert("y1".to_string(), json!(anchor.1 + offset.1));
        }
    }

    if let Some(spec) = to_spec {
        let x2 = value_to_f64(ann.get("x2")).unwrap_or(f64::from(img_w) / 2.0);
        let y2 = value_to_f64(ann.get("y2")).unwrap_or(f64::from(img_h) / 2.0);
        if let Some(target) = resolve_target(&spec, targets, (x2, y2)) {
            let pos = resolve_anchor_pos(
                spec.pos.clone(),
                value_to_string(ann.get("to_pos")),
                defaults,
                "center",
            );
            let offset = resolve_anchor_offset(
                spec.offset,
                parse_offset_value(ann.get("to_offset")),
                defaults,
                (0.0, 0.0),
            );
            let anchor = anchor_point(target.bbox, &pos);
            updated.insert("x2".to_string(), json!(anchor.0 + offset.0));
            updated.insert("y2".to_string(), json!(anchor.1 + offset.1));
        }
    }

    updated
}

fn point_in_rounded_rect(
    px: i32,
    py: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    radius: f64,
) -> bool {
    if px < x0 || px >= x1 || py < y0 || py >= y1 {
        return false;
    }
    if radius <= 0.1 {
        return true;
    }
    let r = radius
        .min(f64::from((x1 - x0).abs()) / 2.0)
        .min(f64::from((y1 - y0).abs()) / 2.0);
    let fx = f64::from(px);
    let fy = f64::from(py);
    let left = f64::from(x0);
    let right = f64::from(x1);
    let top = f64::from(y0);
    let bottom = f64::from(y1);

    if (fx >= left + r && fx <= right - r) || (fy >= top + r && fy <= bottom - r) {
        return true;
    }

    let corners = [
        (left + r, top + r),
        (right - r, top + r),
        (left + r, bottom - r),
        (right - r, bottom - r),
    ];
    corners.iter().any(|(cx, cy)| {
        let dx = fx - cx;
        let dy = fy - cy;
        dx * dx + dy * dy <= r * r
    })
}

fn draw_spotlight_annotation(
    img: &mut RgbaImage,
    ann: &Map<String, Value>,
    scale: f64,
    defaults: &Map<String, Value>,
) {
    let dim_color = parse_color_opt(ann.get("color"))
        .or_else(|| parse_color_opt(ann.get("dim_color")))
        .or_else(|| parse_color_opt(defaults.get("dim_color")))
        .unwrap_or(Rgba([0, 0, 0, 115]));

    let opacity =
        value_to_f64(ann.get("opacity")).or_else(|| value_to_f64(defaults.get("dim_opacity")));
    let final_color = if let Some(alpha_raw) = opacity {
        let alpha = if alpha_raw <= 1.0 {
            (alpha_raw * 255.0).round().clamp(0.0, 255.0) as u8
        } else {
            alpha_raw.round().clamp(0.0, 255.0) as u8
        };
        Rgba([dim_color[0], dim_color[1], dim_color[2], alpha])
    } else {
        dim_color
    };

    let padding = value_to_f64(ann.get("padding"))
        .or_else(|| value_to_f64(defaults.get("dim_padding")))
        .unwrap_or(0.0)
        * scale;
    let radius = value_to_f64(ann.get("radius"))
        .or_else(|| value_to_f64(defaults.get("dim_radius")))
        .unwrap_or(0.0)
        * scale;

    let x = value_to_f64(ann.get("x")).unwrap_or(0.0) - padding;
    let y = value_to_f64(ann.get("y")).unwrap_or(0.0) - padding;
    let w = value_to_f64(ann.get("w")).unwrap_or(0.0) + padding * 2.0;
    let h = value_to_f64(ann.get("h")).unwrap_or(0.0) + padding * 2.0;

    let hole_x0 = x.floor() as i32;
    let hole_y0 = y.floor() as i32;
    let hole_x1 = (x + w).ceil() as i32;
    let hole_y1 = (y + h).ceil() as i32;

    for py in 0..img.height() as i32 {
        for px in 0..img.width() as i32 {
            if point_in_rounded_rect(px, py, hole_x0, hole_y0, hole_x1, hole_y1, radius) {
                continue;
            }
            let dst = *img.get_pixel(px as u32, py as u32);
            img.put_pixel(px as u32, py as u32, blend_pixel(dst, final_color));
        }
    }
}

fn draw_rect_annotation(img: &mut RgbaImage, ann: &Map<String, Value>, scale: f64) {
    let x = value_to_f64(ann.get("x")).unwrap_or(0.0);
    let y = value_to_f64(ann.get("y")).unwrap_or(0.0);
    let w = value_to_f64(ann.get("w")).unwrap_or(0.0);
    let h = value_to_f64(ann.get("h")).unwrap_or(0.0);
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    if let Some(fill) = parse_color_opt(ann.get("fill")) {
        fill_rect_alpha(
            img,
            x.round() as i32,
            y.round() as i32,
            (x + w).round() as i32,
            (y + h).round() as i32,
            fill,
        );
    }

    let stroke = parse_color(ann.get("color"), [255, 59, 48, 255]);
    let width = value_to_usize(ann.get("width"))
        .map(|v| v.max(1) as u32)
        .unwrap_or_else(|| scale_default(3.0, scale, 2));
    let outline_enabled = ann
        .get("outline")
        .map(|v| value_to_bool(v, true))
        .unwrap_or(true);
    let outline_width = value_to_usize(ann.get("outline_width"))
        .map(|v| v.max(1) as u32)
        .unwrap_or_else(|| ((f64::from(width) * 0.6).round() as u32).max(2));
    let outline_color =
        parse_color_opt(ann.get("outline_color")).unwrap_or_else(|| auto_outline_color(stroke));

    let x_u = x.max(0.0).round() as u32;
    let y_u = y.max(0.0).round() as u32;
    let w_u = w.max(1.0).round() as u32;
    let h_u = h.max(1.0).round() as u32;

    if outline_enabled {
        draw_rect_outline(
            img,
            x_u,
            y_u,
            w_u,
            h_u,
            outline_color,
            width + outline_width * 2,
        );
    }
    draw_rect_outline(img, x_u, y_u, w_u, h_u, stroke, width);
}

fn draw_arrow_annotation(img: &mut RgbaImage, ann: &Map<String, Value>, scale: f64) {
    let x1 = value_to_f64(ann.get("x1")).unwrap_or(0.0);
    let y1 = value_to_f64(ann.get("y1")).unwrap_or(0.0);
    let x2 = value_to_f64(ann.get("x2")).unwrap_or(0.0);
    let y2 = value_to_f64(ann.get("y2")).unwrap_or(0.0);
    let color = parse_color(ann.get("color"), [10, 132, 255, 255]);
    let width = value_to_f64(ann.get("width"))
        .unwrap_or_else(|| f64::from(scale_default(3.0, scale, 2)))
        .max(1.0);
    let head_len = value_to_f64(ann.get("head_len"))
        .unwrap_or_else(|| f64::from(scale_default(12.0, scale, 6)))
        .max(2.0);
    let head_width = value_to_f64(ann.get("head_width"))
        .unwrap_or_else(|| f64::from(scale_default(8.0, scale, 5)))
        .max(2.0);

    let outline_enabled = ann
        .get("outline")
        .map(|v| value_to_bool(v, true))
        .unwrap_or(true);
    let outline_width = value_to_f64(ann.get("outline_width"))
        .unwrap_or_else(|| (width * 0.6).round().max(2.0))
        .max(1.0);
    let outline_color =
        parse_color_opt(ann.get("outline_color")).unwrap_or_else(|| auto_outline_color(color));

    if outline_enabled {
        draw_arrow_primitive(
            img,
            x1,
            y1,
            x2,
            y2,
            outline_color,
            width + outline_width * 2.0,
            head_len + outline_width * 2.0,
            head_width + outline_width * 2.0,
        );
    }
    draw_arrow_primitive(img, x1, y1, x2, y2, color, width, head_len, head_width);
}

fn draw_text_annotation(img: &mut RgbaImage, ann: &Map<String, Value>, scale: f64) {
    let text = ann
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if text.is_empty() {
        return;
    }

    let x = value_to_i32(ann.get("x"), 0);
    let y = value_to_i32(ann.get("y"), 0);
    let color = parse_color(ann.get("color"), [255, 255, 255, 255]);
    let size = value_to_usize(ann.get("size"))
        .map(|v| v.max(8) as u32)
        .unwrap_or_else(|| scale_default(14.0, scale, 10));
    let glyph_scale = (size as f64 / 8.0).round().max(1.0) as u32;
    let padding = value_to_usize(ann.get("padding"))
        .map(|v| v as i32)
        .unwrap_or_else(|| scale_default(4.0, scale, 2) as i32);

    let bg_value = ann.get("bg").or_else(|| ann.get("text_bg"));
    if let Some(bg_color) = parse_color_opt(bg_value) {
        let bbox = text_bbox(x, y, &text, glyph_scale);
        fill_rect_alpha(
            img,
            bbox.0 - padding,
            bbox.1 - padding,
            bbox.2 + padding,
            bbox.3 + padding,
            bg_color,
        );
    }

    let outline_enabled = ann
        .get("outline")
        .map(|v| value_to_bool(v, true))
        .unwrap_or(true);
    let outline_width = value_to_usize(ann.get("outline_width"))
        .map(|v| v.max(1) as i32)
        .unwrap_or_else(|| ((size as f64 * 0.12).round() as i32).max(1));
    let outline_color =
        parse_color_opt(ann.get("outline_color")).unwrap_or_else(|| auto_outline_color(color));

    if outline_enabled {
        for dx in -outline_width..=outline_width {
            for dy in -outline_width..=outline_width {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if dx * dx + dy * dy > outline_width * outline_width {
                    continue;
                }
                draw_bitmap_text(img, x + dx, y + dy, &text, outline_color, glyph_scale);
            }
        }
    }

    draw_bitmap_text(img, x, y, &text, color, glyph_scale);
}

fn fit_bbox_luma(
    image_rgb: &image::RgbImage,
    region: (u32, u32, u32, u32),
    threshold: f64,
    target: &str,
    min_pixels: u32,
) -> Option<(u32, u32, u32, u32)> {
    let (x0, y0, x1, y1) = region;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut minx = u32::MAX;
    let mut miny = u32::MAX;
    let mut maxx = 0u32;
    let mut maxy = 0u32;
    let mut count = 0u32;
    let dark = !target.eq_ignore_ascii_case("light");

    for y in y0..y1 {
        for x in x0..x1 {
            let pixel = image_rgb.get_pixel(x, y).0;
            let luma = 0.2126 * f64::from(pixel[0])
                + 0.7152 * f64::from(pixel[1])
                + 0.0722 * f64::from(pixel[2]);
            let matched = if dark {
                luma <= threshold
            } else {
                luma >= threshold
            };
            if matched {
                count += 1;
                minx = minx.min(x);
                miny = miny.min(y);
                maxx = maxx.max(x);
                maxy = maxy.max(y);
            }
        }
    }

    if count < min_pixels.max(1) || minx == u32::MAX {
        return None;
    }
    Some((minx, miny, maxx, maxy))
}

fn fit_bbox_color(
    image_rgb: &image::RgbImage,
    region: (u32, u32, u32, u32),
    color: Rgba<u8>,
    tolerance: f64,
    min_pixels: u32,
) -> Option<(u32, u32, u32, u32)> {
    let (x0, y0, x1, y1) = region;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut minx = u32::MAX;
    let mut miny = u32::MAX;
    let mut maxx = 0u32;
    let mut maxy = 0u32;
    let mut count = 0u32;
    let tol = tolerance.max(0.0);

    for y in y0..y1 {
        for x in x0..x1 {
            let pixel = image_rgb.get_pixel(x, y).0;
            let delta = (i16::from(pixel[0]) - i16::from(color[0]))
                .unsigned_abs()
                .max((i16::from(pixel[1]) - i16::from(color[1])).unsigned_abs())
                .max((i16::from(pixel[2]) - i16::from(color[2])).unsigned_abs())
                as f64;
            if delta <= tol {
                count += 1;
                minx = minx.min(x);
                miny = miny.min(y);
                maxx = maxx.max(x);
                maxy = maxy.max(y);
            }
        }
    }

    if count < min_pixels.max(1) || minx == u32::MAX {
        return None;
    }
    Some((minx, miny, maxx, maxy))
}

fn expand_bbox(
    bbox: Option<(u32, u32, u32, u32)>,
    pad: f64,
    img_w: u32,
    img_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let (x0, y0, x1, y1) = bbox?;
    let p = pad.max(0.0).round() as i64;
    let min_x = (i64::from(x0) - p).clamp(0, i64::from(img_w)) as u32;
    let min_y = (i64::from(y0) - p).clamp(0, i64::from(img_h)) as u32;
    let max_x = (i64::from(x1) + p).clamp(0, i64::from(img_w)) as u32;
    let max_y = (i64::from(y1) + p).clamp(0, i64::from(img_h)) as u32;
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    Some((min_x, min_y, max_x, max_y))
}

fn parse_fit_region(
    fit_region: Option<&Value>,
    ann: &Map<String, Value>,
    img_w: u32,
    img_h: u32,
) -> (u32, u32, u32, u32) {
    let (mut x, mut y, mut w, mut h) = (
        value_to_f64(ann.get("x")),
        value_to_f64(ann.get("y")),
        value_to_f64(ann.get("w")),
        value_to_f64(ann.get("h")),
    );

    if let Some(value) = fit_region {
        match value {
            Value::Object(obj) => {
                x = value_to_f64(obj.get("x"));
                y = value_to_f64(obj.get("y"));
                w = value_to_f64(obj.get("w"));
                h = value_to_f64(obj.get("h"));
            }
            Value::Array(items) if items.len() >= 4 => {
                x = value_to_f64(items.first());
                y = value_to_f64(items.get(1));
                w = value_to_f64(items.get(2));
                h = value_to_f64(items.get(3));
            }
            _ => {}
        }
    }

    let x = x.unwrap_or(0.0);
    let y = y.unwrap_or(0.0);
    let w = w.unwrap_or(f64::from(img_w));
    let h = h.unwrap_or(f64::from(img_h));
    let x0 = clamp_i32(x.round() as i32, 0, img_w as i32) as u32;
    let y0 = clamp_i32(y.round() as i32, 0, img_h as i32) as u32;
    let x1 = clamp_i32((x + w).round() as i32, 0, img_w as i32) as u32;
    let y1 = clamp_i32((y + h).round() as i32, 0, img_h as i32) as u32;
    if x1 <= x0 || y1 <= y0 {
        (0, 0, img_w, img_h)
    } else {
        (x0, y0, x1, y1)
    }
}

fn target_center_from_bbox(bbox: (u32, u32, u32, u32)) -> (f64, f64) {
    (
        f64::from(bbox.0 + bbox.2) / 2.0,
        f64::from(bbox.1 + bbox.3) / 2.0,
    )
}

fn snap_bbox_to_region(
    region: (u32, u32, u32, u32),
    bbox: (u32, u32, u32, u32),
    img_w: u32,
    img_h: u32,
) -> (u32, u32, u32, u32) {
    let region_w = region.2.saturating_sub(region.0);
    let region_h = region.3.saturating_sub(region.1);
    let bbox_w = bbox.2.saturating_sub(bbox.0);
    let bbox_h = bbox.3.saturating_sub(bbox.1);
    if region_w == 0 || region_h == 0 {
        return bbox;
    }
    if bbox_w <= region_w && bbox_h <= region_h {
        let center = target_center_from_bbox(bbox);
        let x0 = clamp_i32(
            (center.0 - f64::from(region_w) / 2.0).round() as i32,
            0,
            img_w as i32,
        ) as u32;
        let y0 = clamp_i32(
            (center.1 - f64::from(region_h) / 2.0).round() as i32,
            0,
            img_h as i32,
        ) as u32;
        let x1 = (x0 + region_w).min(img_w);
        let y1 = (y0 + region_h).min(img_h);
        if x1 > x0 && y1 > y0 {
            return (x0, y0, x1, y1);
        }
    }
    bbox
}

fn resolve_fit_config(ann: &Map<String, Value>, defaults: &Map<String, Value>) -> Option<Value> {
    if let Some(fit) = ann.get("fit") {
        return match fit {
            Value::Bool(v) => {
                if *v {
                    Some(json!({}))
                } else {
                    None
                }
            }
            Value::String(mode) => Some(json!({"mode": mode})),
            Value::Object(obj) => Some(Value::Object(obj.clone())),
            _ => None,
        };
    }

    let auto_fit = defaults
        .get("auto_fit")
        .map(|v| value_to_bool(v, true))
        .unwrap_or(true);
    if !auto_fit {
        return None;
    }

    let mut fit = Map::new();
    fit.insert(
        "mode".to_string(),
        defaults
            .get("fit_mode")
            .cloned()
            .unwrap_or_else(|| json!("luma")),
    );
    for key in [
        "fit_threshold",
        "fit_target",
        "fit_tolerance",
        "fit_color",
        "fit_pad",
        "fit_min_pixels",
        "fit_min_coverage",
    ] {
        if let Some(value) = defaults.get(key).cloned() {
            let out_key = key.trim_start_matches("fit_").to_string();
            fit.insert(out_key, value);
        }
    }
    Some(Value::Object(fit))
}

fn apply_fit(
    ann: &Map<String, Value>,
    image_rgb: &image::RgbImage,
    img_w: u32,
    img_h: u32,
    defaults: &Map<String, Value>,
) -> Map<String, Value> {
    let fit_value = resolve_fit_config(ann, defaults);
    let Some(fit_value) = fit_value else {
        return ann.clone();
    };
    let Value::Object(fit) = fit_value else {
        return ann.clone();
    };

    let mode = value_to_string(fit.get("mode"))
        .unwrap_or_else(|| "luma".to_string())
        .to_ascii_lowercase();
    let region = parse_fit_region(fit.get("region"), ann, img_w, img_h);
    let min_pixels = value_to_f64(fit.get("min_pixels")).unwrap_or(30.0).max(1.0) as u32;
    let min_coverage = value_to_f64(fit.get("min_coverage"))
        .unwrap_or(0.6)
        .max(0.0);

    let mut bbox = if mode == "luma" {
        let threshold = value_to_f64(fit.get("threshold")).unwrap_or(160.0);
        let target = value_to_string(fit.get("target")).unwrap_or_else(|| "dark".to_string());
        fit_bbox_luma(image_rgb, region, threshold, &target, min_pixels)
    } else if mode == "color" {
        let target_color = parse_color_opt(fit.get("color").or_else(|| fit.get("target_color")));
        let Some(color) = target_color else {
            return ann.clone();
        };
        let tolerance = value_to_f64(fit.get("tolerance")).unwrap_or(18.0);
        fit_bbox_color(image_rgb, region, color, tolerance, min_pixels)
    } else {
        return ann.clone();
    };

    let pad = value_to_f64(fit.get("pad")).unwrap_or(0.0);
    bbox = expand_bbox(bbox, pad, img_w, img_h);
    let Some(mut bbox) = bbox else {
        return ann.clone();
    };

    let region_area = f64::from(region.2.saturating_sub(region.0).max(1))
        * f64::from(region.3.saturating_sub(region.1).max(1));
    let bbox_area = f64::from(bbox.2.saturating_sub(bbox.0).max(1))
        * f64::from(bbox.3.saturating_sub(bbox.1).max(1));
    if bbox_area / region_area < min_coverage {}
    bbox = snap_bbox_to_region(region, bbox, img_w, img_h);

    let mut updated = ann.clone();
    updated.insert("x".to_string(), json!(bbox.0));
    updated.insert("y".to_string(), json!(bbox.1));
    updated.insert("w".to_string(), json!(bbox.2.saturating_sub(bbox.0)));
    updated.insert("h".to_string(), json!(bbox.3.saturating_sub(bbox.1)));
    updated
}

fn activate_process_window(process: &str) -> QueryDiagnostic {
    if !cfg!(target_os = "macos") {
        return QueryDiagnostic {
            ok: false,
            attempts: 0,
            error_code: Some("unsupported_platform".to_string()),
            message: Some("activation requires macOS".to_string()),
        };
    }

    let escaped = process.replace('"', "\\\"");
    let script = format!("try\ntell application \"{escaped}\" to activate\nreturn \"ok\"\non error errMsg number errNum\nreturn \"err:\" & errNum & \":\" & errMsg\nend try");
    let (stdout, mut diag) = run_osascript_with_retry(&script, &[], 1, 40);
    if let Some(text) = stdout {
        if text.starts_with("err:") {
            let code = text
                .split(':')
                .nth(1)
                .map(|v| format!("osascript_{v}"))
                .unwrap_or_else(|| "activation_failed".to_string());
            diag.ok = false;
            diag.error_code = Some(code);
            diag.message = Some(text);
        } else {
            diag.ok = true;
            diag.error_code = None;
            diag.message = Some("activated".to_string());
        }
    }
    diag
}

fn query_window_probe(process: &str) -> WindowProbe {
    const MIN_USABLE_WINDOW_WIDTH: i64 = 220;
    const MIN_USABLE_WINDOW_HEIGHT: i64 = 140;
    const MIN_USABLE_WINDOW_AREA: i64 = 40_000;

    let mut probe = WindowProbe {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        title: None,
        selected_index: None,
        candidate_count: 0,
        usable_count: 0,
        selection_mode: "none".to_string(),
        usable: false,
        min_width: MIN_USABLE_WINDOW_WIDTH,
        min_height: MIN_USABLE_WINDOW_HEIGHT,
        min_area: MIN_USABLE_WINDOW_AREA,
        diagnostics: QueryDiagnostic {
            ok: false,
            attempts: 0,
            error_code: Some("window_query_not_started".to_string()),
            message: Some("window query not executed".to_string()),
        },
    };

    if !cfg!(target_os = "macos") {
        probe.diagnostics = QueryDiagnostic {
            ok: false,
            attempts: 0,
            error_code: Some("unsupported_platform".to_string()),
            message: Some("window queries require macOS".to_string()),
        };
        return probe;
    }

    let script = r#"
on cleanText(v)
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
end cleanText

on emitLine(idxVal, xVal, yVal, wVal, hVal, titleVal)
  return (idxVal as text) & tab & (xVal as text) & tab & (yVal as text) & tab & (wVal as text) & tab & (hVal as text) & tab & my cleanText(titleVal)
end emitLine

on run argv
  set procName to item 1 of argv
  tell application "System Events"
    if not (exists process procName) then
      error "process_not_found"
    end if
    tell process procName
      set winRefs to windows
      if (count of winRefs) is 0 then
        error "no_windows"
      end if
      set linesOut to {}
      set idx to 0
      repeat with winRef in winRefs
        set idx to idx + 1
        try
          set p to position of winRef
          set s to size of winRef
          set wx to item 1 of p
          set wy to item 2 of p
          set ww to item 1 of s
          set wh to item 2 of s
          if ww > 0 and wh > 0 then
            set wt to ""
            try
              set wt to name of winRef
            end try
            set end of linesOut to my emitLine(idx, wx, wy, ww, wh, wt)
          end if
        end try
      end repeat
      if (count of linesOut) is 0 then
        error "no_window_bounds"
      end if
      set AppleScript's text item delimiters to linefeed
      set outText to linesOut as text
      set AppleScript's text item delimiters to ""
      return outText
    end tell
  end tell
end run
"#;

    let args = vec![process.to_string()];
    let (raw_lines, raw_diag) = run_osascript_with_retry(script, &args, 3, 120);
    let attempts = raw_diag.attempts.max(1);

    let Some(lines) = raw_lines else {
        probe.diagnostics = QueryDiagnostic {
            ok: false,
            attempts,
            error_code: raw_diag
                .error_code
                .clone()
                .or(Some("window_query_empty".to_string())),
            message: raw_diag
                .message
                .clone()
                .or(Some("window bounds missing".to_string())),
        };
        return probe;
    };

    let candidates = parse_window_candidates(&lines);
    if candidates.is_empty() {
        probe.diagnostics = QueryDiagnostic {
            ok: false,
            attempts,
            error_code: Some("window_bounds_parse_failed".to_string()),
            message: Some("window bounds output was present but parse failed".to_string()),
        };
        return probe;
    }

    probe.candidate_count = candidates.len();
    let (selected, selection_mode, usable_count) = select_window_candidate(
        &candidates,
        MIN_USABLE_WINDOW_WIDTH,
        MIN_USABLE_WINDOW_HEIGHT,
        MIN_USABLE_WINDOW_AREA,
    );
    probe.x = selected.x;
    probe.y = selected.y;
    probe.w = selected.w;
    probe.h = selected.h;
    probe.title = selected.title.clone();
    probe.selected_index = Some(selected.index);
    probe.selection_mode = selection_mode.to_string();
    probe.usable_count = usable_count;
    probe.usable = selected.w >= MIN_USABLE_WINDOW_WIDTH
        && selected.h >= MIN_USABLE_WINDOW_HEIGHT
        && selected.w.saturating_mul(selected.h) >= MIN_USABLE_WINDOW_AREA;

    let selection_note = format!(
        "selected window {} ({}) {}x{}, candidates={} usable={}",
        selected.index,
        selection_mode,
        selected.w,
        selected.h,
        probe.candidate_count,
        probe.usable_count
    );

    probe.diagnostics = QueryDiagnostic {
        ok: true,
        attempts,
        error_code: None,
        message: Some(selection_note),
    };
    probe
}

fn run_osascript_with_retry(
    script: &str,
    args: &[String],
    attempts: u32,
    delay_ms: u64,
) -> (Option<String>, QueryDiagnostic) {
    if !cfg!(target_os = "macos") {
        return (
            None,
            QueryDiagnostic {
                ok: false,
                attempts: 0,
                error_code: Some("unsupported_platform".to_string()),
                message: Some("osascript requires macOS".to_string()),
            },
        );
    }

    let max_attempts = attempts.max(1);
    let timeout_ms = 450u64;
    let mut last_code = Some("osascript_no_output".to_string());
    let mut last_message = Some("osascript returned empty output".to_string());

    for attempt in 1..=max_attempts {
        let mut cmd = Command::new("osascript");
        cmd.arg("-e").arg(script);
        if !args.is_empty() {
            cmd.arg("--");
            for arg in args {
                cmd.arg(arg);
            }
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        match cmd.spawn() {
            Ok(mut child) => match child.wait_timeout(Duration::from_millis(timeout_ms)) {
                Ok(Some(_)) => match child.wait_with_output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        if output.status.success() && !stdout.is_empty() {
                            return (
                                Some(stdout),
                                QueryDiagnostic {
                                    ok: true,
                                    attempts: attempt,
                                    error_code: None,
                                    message: None,
                                },
                            );
                        }

                        if output.status.success() {
                            last_code = Some("osascript_empty_stdout".to_string());
                            last_message =
                                Some("osascript succeeded but returned empty output".to_string());
                        } else {
                            let code = output.status.code().unwrap_or(1);
                            last_code = Some(format!("osascript_exit_{code}"));
                            last_message = Some(if stderr.is_empty() {
                                format!("osascript failed with status {code}")
                            } else {
                                stderr
                            });
                        }
                    }
                    Err(err) => {
                        last_code = Some("osascript_wait_output_failed".to_string());
                        last_message = Some(err.to_string());
                    }
                },
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    last_code = Some("osascript_timeout".to_string());
                    last_message = Some(format!(
                        "osascript timed out after {}ms (attempt {attempt}/{max_attempts})",
                        timeout_ms
                    ));
                }
                Err(err) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    last_code = Some("osascript_wait_timeout_failed".to_string());
                    last_message = Some(err.to_string());
                }
            },
            Err(err) => {
                last_code = Some("osascript_spawn_failed".to_string());
                last_message = Some(err.to_string());
            }
        }

        if attempt < max_attempts {
            let backoff = delay_ms.saturating_mul(u64::from(attempt));
            thread::sleep(Duration::from_millis(backoff.max(10)));
        }
    }

    (
        None,
        QueryDiagnostic {
            ok: false,
            attempts: max_attempts,
            error_code: last_code,
            message: last_message,
        },
    )
}

fn parse_window_candidates(raw: &str) -> Vec<WindowCandidate> {
    let mut items = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() < 5 {
            continue;
        }
        let index = match parts[0].trim().parse::<usize>() {
            Ok(v) if v > 0 => v,
            _ => continue,
        };
        let x = match parts[1].trim().parse::<f64>() {
            Ok(v) => v.round() as i64,
            _ => continue,
        };
        let y = match parts[2].trim().parse::<f64>() {
            Ok(v) => v.round() as i64,
            _ => continue,
        };
        let w = match parts[3].trim().parse::<f64>() {
            Ok(v) => v.round() as i64,
            _ => continue,
        };
        let h = match parts[4].trim().parse::<f64>() {
            Ok(v) => v.round() as i64,
            _ => continue,
        };
        if w <= 0 || h <= 0 {
            continue;
        }
        let title = parts
            .get(5)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(ToString::to_string);
        items.push(WindowCandidate {
            index,
            x,
            y,
            w,
            h,
            title,
        });
    }
    items
}

fn select_window_candidate<'a>(
    candidates: &'a [WindowCandidate],
    min_width: i64,
    min_height: i64,
    min_area: i64,
) -> (&'a WindowCandidate, &'static str, usize) {
    const MIN_REASONABLE_XY: i64 = -5_000;
    const MAX_REASONABLE_XY: i64 = 50_000;

    fn area(candidate: &WindowCandidate) -> i64 {
        candidate.w.saturating_mul(candidate.h)
    }

    fn cmp_window(a: &WindowCandidate, b: &WindowCandidate) -> std::cmp::Ordering {
        area(b)
            .cmp(&area(a))
            .then_with(|| b.w.cmp(&a.w))
            .then_with(|| b.h.cmp(&a.h))
            .then_with(|| a.index.cmp(&b.index))
    }

    let is_usable = |candidate: &WindowCandidate| {
        candidate.w >= min_width
            && candidate.h >= min_height
            && area(candidate) >= min_area
            && candidate.x >= MIN_REASONABLE_XY
            && candidate.y >= MIN_REASONABLE_XY
            && candidate.x <= MAX_REASONABLE_XY
            && candidate.y <= MAX_REASONABLE_XY
    };

    let mut usable: Vec<&WindowCandidate> = candidates.iter().filter(|c| is_usable(c)).collect();
    usable.sort_by(|a, b| cmp_window(a, b));
    let usable_count = usable.len();
    if let Some(candidate) = usable.first().copied() {
        return (candidate, "largest_usable", usable_count);
    }

    let mut all: Vec<&WindowCandidate> = candidates.iter().collect();
    all.sort_by(|a, b| cmp_window(a, b));
    if let Some(candidate) = all.first().copied() {
        return (candidate, "largest_any", usable_count);
    }

    // parse_window_candidates guarantees non-empty when this is called.
    (&candidates[0], "window_1", usable_count)
}

fn frontmost_app_name() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    run_osascript_with_retry(
        "tell application \"System Events\" to get name of first process whose frontmost is true",
        &[],
        1,
        20,
    )
    .0
}

fn query_ax_tree(process: &str, depth: u32) -> AxQueryResult {
    if !cfg!(target_os = "macos") {
        return AxQueryResult {
            elements: Vec::new(),
            tree: Vec::new(),
            diagnostics: QueryDiagnostic {
                ok: false,
                attempts: 0,
                error_code: Some("unsupported_platform".to_string()),
                message: Some("AX tree extraction requires macOS".to_string()),
            },
            warnings: vec!["ax-tree is only available on macOS; emitted empty payload".to_string()],
        };
    }

    let script = r#"
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
"#;

    let args = vec![process.to_string(), depth.to_string()];
    let (raw_lines, diagnostics) = run_osascript_with_retry(script, &args, 2, 80);
    let mut warnings = Vec::new();

    let Some(lines) = raw_lines else {
        if let Some(code) = diagnostics.error_code.as_deref() {
            warnings.push(format!("ax_tree_query:{code}"));
        }
        if let Some(message) = diagnostics.message.as_deref() {
            warnings.push(format!("AX tree unavailable: {message}"));
        }
        return AxQueryResult {
            elements: Vec::new(),
            tree: Vec::new(),
            diagnostics,
            warnings,
        };
    };

    let flat_nodes = parse_ax_lines(&lines);
    let elements: Vec<Value> = flat_nodes.iter().map(ax_element_value).collect();
    let tree = ax_tree_values(&flat_nodes);

    if flat_nodes.is_empty() {
        warnings.push("AX tree query returned no elements".to_string());
    }

    AxQueryResult {
        elements,
        tree,
        diagnostics,
        warnings,
    }
}

fn parse_ax_lines(raw: &str) -> Vec<AxFlatNode> {
    let mut rows = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts: Vec<&str> = line.split('\t').collect();
        while parts.len() < 9 {
            parts.push("");
        }

        let depth = parts[0].trim().parse::<usize>().unwrap_or(0);
        let class_name = parts[1].trim().to_string();
        let name = (!parts[2].trim().is_empty()).then(|| parts[2].trim().to_string());
        let role_description = (!parts[3].trim().is_empty()).then(|| parts[3].trim().to_string());
        let enabled = (!parts[4].trim().is_empty()).then(|| parts[4].trim().to_string());

        let x = parts[5]
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| v.round() as i64);
        let y = parts[6]
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| v.round() as i64);
        let w = parts[7]
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| v.round() as i64);
        let h = parts[8]
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| v.round() as i64);
        let bounds = match (x, y, w, h) {
            (Some(x), Some(y), Some(w), Some(h)) => Some((x, y, w, h)),
            _ => None,
        };

        rows.push(AxFlatNode {
            index,
            depth,
            class_name,
            name,
            role_description,
            enabled,
            bounds,
        });
    }
    rows
}

fn ax_bounds_value(bounds: Option<(i64, i64, i64, i64)>) -> Value {
    match bounds {
        Some((x, y, w, h)) => json!({
            "x": x,
            "y": y,
            "w": w,
            "h": h,
            "units": "pt",
        }),
        None => Value::Null,
    }
}

fn ax_element_value(node: &AxFlatNode) -> Value {
    json!({
        "index": node.index,
        "depth": node.depth,
        "class": node.class_name,
        "name": node.name,
        "role_description": node.role_description,
        "enabled": node.enabled,
        "bounds": ax_bounds_value(node.bounds),
    })
}

fn ax_tree_node_value(node: &AxTreeNode) -> Value {
    let children: Vec<Value> = node.children.iter().map(ax_tree_node_value).collect();
    json!({
        "index": node.index,
        "class": node.class_name,
        "name": node.name,
        "role_description": node.role_description,
        "enabled": node.enabled,
        "bounds": ax_bounds_value(node.bounds),
        "children": children,
    })
}

fn ax_tree_values(rows: &[AxFlatNode]) -> Vec<Value> {
    let mut roots: Vec<AxTreeNode> = Vec::new();
    let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();

    for row in rows {
        let node = AxTreeNode {
            index: row.index,
            class_name: row.class_name.clone(),
            name: row.name.clone(),
            role_description: row.role_description.clone(),
            enabled: row.enabled.clone(),
            bounds: row.bounds,
            children: Vec::new(),
        };

        while stack
            .last()
            .map(|(depth, _)| *depth >= row.depth)
            .unwrap_or(false)
        {
            stack.pop();
        }

        if let Some((_, parent_path)) = stack.last().cloned() {
            if let Some(parent) = ax_tree_node_mut(&mut roots, &parent_path) {
                parent.children.push(node);
                let mut node_path = parent_path;
                node_path.push(parent.children.len() - 1);
                stack.push((row.depth, node_path));
            } else {
                roots.push(node);
                stack.push((row.depth, vec![roots.len() - 1]));
            }
        } else {
            roots.push(node);
            stack.push((row.depth, vec![roots.len() - 1]));
        }
    }

    roots.iter().map(ax_tree_node_value).collect()
}

fn ax_tree_node_mut<'a>(nodes: &'a mut [AxTreeNode], path: &[usize]) -> Option<&'a mut AxTreeNode> {
    let (head, tail) = path.split_first()?;
    let node = nodes.get_mut(*head)?;
    if tail.is_empty() {
        Some(node)
    } else {
        ax_tree_node_mut(node.children.as_mut_slice(), tail)
    }
}

fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    ensure_parent_dir(dst)?;
    fs::copy(src, dst)
        .with_context(|| format!("failed to copy {} -> {}", src.display(), dst.display()))?;
    Ok(())
}

fn write_json_pretty(path: &Path, value: &Value) -> Result<()> {
    ensure_parent_dir(path)?;
    let raw = serde_json::to_string_pretty(value)?;
    fs::write(path, raw).with_context(|| format!("failed to write JSON: {}", path.display()))?;
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directory: {}", parent.display())
            })?;
        }
    }
    Ok(())
}

fn default_sidecar_for(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    parent.join(format!("{stem}.json"))
}

fn out_root() -> PathBuf {
    env::var("CVLP_OUT_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var("CVLP_TMP_DIR")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from(".codex-visual-loop"))
}

fn abs_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}

fn sanitize_baseline_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch);
        } else if matches!(ch, ' ' | '/' | ':') {
            out.push('_');
        }
    }
    if out.is_empty() {
        "baseline".to_string()
    } else {
        out
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() || matches!(lower, '.' | '_' | '-') {
            out.push(lower);
        } else if lower.is_ascii_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "app".to_string()
    } else {
        out
    }
}

fn timestamp_compact() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

fn timestamp_iso() -> String {
    Utc::now().to_rfc3339()
}

fn round_to(v: f64, digits: u32) -> f64 {
    let factor = 10f64.powi(digits as i32);
    (v * factor).round() / factor
}

fn command_exists(name: &str) -> bool {
    Command::new("bash")
        .arg("-lc")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rel_units_convert_to_pixels() {
        let mut ann = Map::new();
        ann.insert("units".to_string(), json!("rel"));
        ann.insert("x".to_string(), json!(0.1));
        ann.insert("y".to_string(), json!(0.2));
        ann.insert("w".to_string(), json!(0.5));
        ann.insert("h".to_string(), json!(0.4));
        let defaults = Map::new();
        resolve_annotation_units(&mut ann, 200, 100, &defaults);

        assert_eq!(ann.get("x").and_then(Value::as_f64), Some(20.0));
        assert_eq!(ann.get("y").and_then(Value::as_f64), Some(20.0));
        assert_eq!(ann.get("w").and_then(Value::as_f64), Some(100.0));
        assert_eq!(ann.get("h").and_then(Value::as_f64), Some(40.0));
    }

    #[test]
    fn diff_detects_change_regions() {
        let mut gray = vec![0u8; 100 * 60];
        for y in 10..30 {
            for x in 20..40 {
                gray[y * 100 + x] = 255;
            }
        }
        let regions = extract_change_regions(&gray, 100, 60, 1, 10, 2, 8);
        assert!(!regions.is_empty());
        let first = &regions[0];
        assert!(first.x <= 20);
        assert!(first.y <= 10);
        assert!(first.w >= 20);
        assert!(first.h >= 20);
    }

    #[test]
    fn writes_json_pretty() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a").join("b.json");
        write_json_pretty(&target, &json!({"ok": true})).unwrap();
        assert!(target.exists());
    }

    #[test]
    fn parse_window_candidates_reads_tsv_rows() {
        let raw = "1\t0\t38\t902\t1079\tMain\n2\t8\t8\t30\t23\tTiny";
        let parsed = parse_window_candidates(raw);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].index, 1);
        assert_eq!(parsed[0].w, 902);
        assert_eq!(parsed[1].h, 23);
    }

    #[test]
    fn select_window_candidate_prefers_largest_usable() {
        let windows = vec![
            WindowCandidate {
                index: 1,
                x: 0,
                y: 0,
                w: 30,
                h: 23,
                title: Some("tiny".to_string()),
            },
            WindowCandidate {
                index: 2,
                x: 10,
                y: 10,
                w: 640,
                h: 480,
                title: Some("main".to_string()),
            },
            WindowCandidate {
                index: 3,
                x: 20,
                y: 20,
                w: 800,
                h: 600,
                title: Some("largest".to_string()),
            },
        ];
        let (selected, mode, usable_count) = select_window_candidate(&windows, 220, 140, 40_000);
        assert_eq!(selected.index, 3);
        assert_eq!(mode, "largest_usable");
        assert_eq!(usable_count, 2);
    }

    #[test]
    fn select_window_candidate_uses_largest_any_when_all_tiny() {
        let windows = vec![
            WindowCandidate {
                index: 1,
                x: 0,
                y: 0,
                w: 30,
                h: 23,
                title: None,
            },
            WindowCandidate {
                index: 2,
                x: 0,
                y: 0,
                w: 120,
                h: 90,
                title: None,
            },
        ];
        let (selected, mode, usable_count) = select_window_candidate(&windows, 220, 140, 40_000);
        assert_eq!(selected.index, 2);
        assert_eq!(mode, "largest_any");
        assert_eq!(usable_count, 0);
    }
}
