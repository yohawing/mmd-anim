use std::{
    collections::HashSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand, ValueEnum};

mod commands;
mod mmd_dumper_oracle;
mod schema;

// ---------------------------------------------------------------------------
// Clap CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "mmd-anim",
    version,
    about = "CLI diagnostics and roundtrip tools for mmd-anim\n\nExit codes: 0 = success, 1 = runtime error, 2 = usage error",
    after_help = "Quick start:\n  mmd-anim inspect model.pmx              Show model summary\n  mmd-anim import model.pmx motion.vmd    Import and inspect pair\n  mmd-anim roundtrip model.pmx            Verify parse-export stability",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect an MMD asset without changing it.
    #[command(
        long_about = "Parse an MMD asset and print a compact summary by default.\nUse this for quick format triage, JSON dumps, or PMX IK solver inspection.\nFor detailed rig structure (IK chains, grants, bone hierarchy), use the rig command instead.\n\nSupported formats: .pmx, .pmd, .vmd, .pmm",
        after_help = "Examples:\n  mmd-anim inspect model.pmx\n  mmd-anim inspect motion.vmd --json\n  mmd-anim inspect model.pmx --ik"
    )]
    Inspect {
        /// Path to the asset to inspect
        asset: PathBuf,
        /// Output parsed data as JSON
        #[arg(long)]
        json: bool,
        /// Show PMX IK solver details
        #[arg(long)]
        ik: bool,
        /// Write inspect output to a file instead of stdout
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Import model and optional motion into runtime structures.
    #[command(
        long_about = "Run the runtime importer for a model, or a model/motion pair.\nUse this when checking runtime names, clip build stats, or a single evaluated frame.\n\nSupported formats: .pmx + .vmd, .pmd + .vmd",
        after_help = "Examples:\n  mmd-anim import model.pmx\n  mmd-anim import model.pmx motion.vmd --clip\n  mmd-anim import model.pmx motion.vmd --frame 120\n    (unit: MMD coordinate)"
    )]
    Import {
        /// Path to the PMX/PMD model file
        model: PathBuf,
        /// Optional path to the VMD motion file
        motion: Option<PathBuf>,
        /// Request JSON output where supported
        #[arg(long)]
        json: bool,
        /// Show clip build statistics for a model/motion pair
        #[arg(long)]
        clip: bool,
        /// Evaluate a single frame for a model/motion pair
        #[arg(long)]
        frame: Option<f32>,
    },

    /// Verify parse/export/re-parse stability.
    #[command(
        long_about = "Parse an asset, export it, then re-parse the exported bytes.\nUse this to verify that the parser and exporter produce consistent output.\nJSON reports use jsonBytes for the JSON serialized byte count when --via-json is set.\n\nSupported formats: .pmx, .pmd, .vmd, .pmm",
        after_help = "Examples:\n  mmd-anim roundtrip model.pmx\n  mmd-anim roundtrip motion.vmd --json\n  mmd-anim roundtrip model.pmx --via-json"
    )]
    Roundtrip {
        /// Path to the asset to roundtrip
        asset: PathBuf,
        /// Output roundtrip report as JSON
        #[arg(long)]
        json: bool,
        /// Roundtrip through JSON serialization before binary export
        #[arg(long)]
        via_json: bool,
    },

    /// Inspect PMX rig structure.
    #[command(
        long_about = "Inspect IK chains, grant/append transforms, and deform layer distribution.\nUse this for detailed rig analysis; for a quick file overview, use inspect instead.\n\nSupported formats: .pmx only",
        after_help = "Examples:\n  mmd-anim rig model.pmx\n  mmd-anim rig model.pmx --bones\n  mmd-anim rig model.pmx --json --bones"
    )]
    Rig {
        /// Path to the PMX model file
        model: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Include the full bone list
        #[arg(long)]
        bones: bool,
    },

    /// Benchmark runtime evaluation.
    #[command(
        long_about = "Benchmark a PMX/VMD pair by default, or synthetic runtime data with --synthetic.\nUse this for local performance checks around import, clip build, and evaluation.\n\nPair mode: <model.pmx> <motion.vmd> [start-frame] [frame-count] [step]\n  Flags: --no-ik, --ik-tolerance <value>, --ik-max-iterations-cap <count>\n\nSynthetic mode: --synthetic [models] [bones] [frames] [--json]\n  Defaults: models=1, bones=32, frames=1000\n\nSupported formats: .pmx + .vmd",
        after_help = "Examples:\n  mmd-anim bench model.pmx motion.vmd\n  mmd-anim bench model.pmx motion.vmd 0 240 1 --no-ik\n  mmd-anim bench --synthetic\n  mmd-anim bench --synthetic 4 64 2000\n  mmd-anim bench --synthetic 4 64 2000 --json"
    )]
    Bench {
        /// Path to the PMX model file
        model: Option<PathBuf>,
        /// Path to the VMD motion file
        motion: Option<PathBuf>,
        /// Run the synthetic benchmark instead of a PMX/VMD pair
        #[arg(long)]
        synthetic: bool,
        /// Additional pair or synthetic benchmark arguments
        #[arg(
            value_name = "ARGS",
            trailing_var_arg = true,
            allow_hyphen_values = true
        )]
        extra_args: Vec<String>,
    },

    /// Verify oracle, golden, parser, or numeric comparison data.
    #[command(
        long_about = "Run comparison and oracle diagnostics from a manifest, oracle file, or golden root.\nWhen --mode is omitted, verify reads the target as an oracle JSONL summary file.\nMode inputs:\n  numeric: manifest JSON for numeric model/motion/oracle cases\n  camera: manifest JSON for camera comparison cases\n  ik: golden root directory containing IK fixture/oracle data\n  parser: golden root directory containing parser golden data\n  omitted: oracle JSONL summary file\nUse this for numeric, camera, IK, parser, and focused diagnosis workflows.",
        after_help = "Examples:\n  mmd-anim verify oracle.jsonl\n  mmd-anim verify manifest.json --mode numeric\n  mmd-anim verify camera-manifest.json --mode camera\n  mmd-anim verify golden-root --mode ik\n  mmd-anim verify golden-root --mode ik --compare\n  mmd-anim verify golden-root --mode parser\n  mmd-anim verify manifest.json --mode numeric --diagnose case-a 120 左足ＩＫ"
    )]
    Verify {
        /// Path to a manifest, oracle JSONL file, or golden root directory
        target: PathBuf,
        /// Verification mode
        #[arg(long, value_enum)]
        mode: Option<VerifyMode>,
        /// Diagnose a specific case/frame, with optional bone name
        #[arg(long, num_args = 2..=3, value_names = ["CASE", "FRAME", "BONE"])]
        diagnose: Option<Vec<String>>,
        /// Compare IK golden data instead of printing the IK summary
        #[arg(long)]
        compare: bool,
        /// Request JSON output where supported
        #[arg(long)]
        json: bool,
        /// Numeric diagnosis evaluation frame override
        #[arg(long)]
        eval_frame: Option<f32>,
        /// IK comparison/diagnosis sample frame offset
        #[arg(long)]
        sample_frame_offset: Option<f32>,
    },

    /// Patch PMM fields in place to a new output file.
    #[command(
        long_about = "Rewrite selected PMM document fields while preserving the rest of the file.\nUse --model-path when a PMM document model slot points at the wrong model path.\nUse --frame-range when the scene timeline current frame, begin/end frame, or range enabled flags need correction.\nThe flag structure is intentionally stable: --model-path takes <idx> <path> <out>, and --frame-range takes <out> plus one or more frame-range options.",
        after_help = "Examples:\n  mmd-anim patch scene.pmm --model-path 0 model.pmx out.pmm\n  mmd-anim patch scene.pmm --frame-range out.pmm --current-frame 120\n  mmd-anim patch scene.pmm --frame-range out.pmm --begin-frame 0 --end-frame 240\n  mmd-anim patch scene.pmm --frame-range out.pmm --begin-frame-enabled true --end-frame-enabled true"
    )]
    Patch {
        /// Path to the input PMM file
        pmm: PathBuf,
        /// Patch a document model path: <idx> <path> <out>
        #[arg(long, num_args = 3, value_names = ["IDX", "PATH", "OUT"])]
        model_path: Option<Vec<String>>,
        /// Patch scene frame range settings and write to this output path
        #[arg(long, value_name = "OUT")]
        frame_range: Option<PathBuf>,
        /// Set current frame index
        #[arg(long)]
        current_frame: Option<i32>,
        /// Set current frame text field index
        #[arg(long)]
        current_frame_text: Option<i32>,
        /// Set begin frame index
        #[arg(long)]
        begin_frame: Option<i32>,
        /// Set end frame index
        #[arg(long)]
        end_frame: Option<i32>,
        /// Enable or disable begin frame range
        #[arg(long)]
        begin_frame_enabled: Option<String>,
        /// Enable or disable end frame range
        #[arg(long)]
        end_frame_enabled: Option<String>,
    },

    /// Export an asset to another binary file.
    #[command(
        long_about = "Write an MMD asset to an output path, optionally starting from JSON.\nWith --from-json, the input must be UTF-8 JSON text and the output extension selects the binary format.\nThe JSON shape is the raw parsed DTO emitted by `mmd-anim inspect <asset> --json`, for example PmxParsedModel for .pmx, PmdParsedModel for .pmd, or VmdParsedAnimation for .vmd.\nUse this for parser/exporter smoke checks and JSON-to-binary conversion.\n\nSupported formats: .pmx, .pmd, .vmd",
        after_help = "Examples:\n  mmd-anim export input.vmd output.vmd\n  mmd-anim export input.json output.vmd --from-json"
    )]
    Export {
        /// Path to the input asset or JSON file
        input: PathBuf,
        /// Path to the output asset file
        output: PathBuf,
        /// Treat input as JSON and export binary format
        #[arg(long)]
        from_json: bool,
    },

    /// Build a PMM scene from a model and motion.
    #[command(
        name = "build-pmm",
        long_about = "Create a PMM scene from a PMX model and VMD motion.\nUse this when preparing MMD GUI-compatible scenes from runtime fixtures.\n\nSupported formats: .pmx + .vmd → .pmm",
        after_help = "Examples:\n  mmd-anim build-pmm model.pmx motion.vmd scene.pmm\n  mmd-anim build-pmm ./model.pmx ./motion.vmd ./out/scene.pmm"
    )]
    BuildPmm {
        /// Path to the PMX model file
        model: PathBuf,
        /// Path to the VMD motion file
        motion: PathBuf,
        /// Path to the output PMM file
        output: PathBuf,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum VerifyMode {
    Numeric,
    Camera,
    Ik,
    Parser,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result: Result<ExitCode, Box<dyn std::error::Error>> = match cli.command {
        None => {
            println!("mmd-anim {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }

        Some(Commands::Inspect {
            asset,
            json,
            ik,
            output,
        }) => dispatch_inspect(&asset, json, ik, output.as_deref()),
        Some(Commands::Import {
            model,
            motion,
            json,
            clip,
            frame,
        }) => dispatch_import(&model, motion.as_deref(), json, clip, frame),
        Some(Commands::Roundtrip {
            asset,
            json,
            via_json,
        }) => dispatch_roundtrip(&asset, json, via_json),
        Some(Commands::Rig { model, json, bones }) => {
            commands::rig::rig_inspect(&model, json, bones)
        }
        Some(Commands::Bench {
            model,
            motion,
            synthetic,
            extra_args,
        }) => dispatch_bench(model, motion, synthetic, extra_args),
        Some(Commands::Verify {
            target,
            mode,
            diagnose,
            compare,
            json,
            eval_frame,
            sample_frame_offset,
        }) => dispatch_verify(
            &target,
            mode,
            diagnose,
            compare,
            json,
            eval_frame,
            sample_frame_offset,
        ),
        Some(Commands::Patch {
            pmm,
            model_path,
            frame_range,
            current_frame,
            current_frame_text,
            begin_frame,
            end_frame,
            begin_frame_enabled,
            end_frame_enabled,
        }) => dispatch_patch(
            &pmm,
            model_path,
            frame_range,
            PmmFrameRangeArgs {
                current_frame,
                current_frame_text,
                begin_frame,
                end_frame,
                begin_frame_enabled,
                end_frame_enabled,
            },
        ),
        Some(Commands::Export {
            input,
            output,
            from_json,
        }) => dispatch_export(&input, &output, from_json),
        Some(Commands::BuildPmm {
            model,
            motion,
            output,
        }) => commands::export::export_pmm_scene(&model, &motion, &output),
    };

    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}", format_cli_error(error.as_ref()));
            ExitCode::FAILURE
        }
    }
}

fn dispatch_inspect(
    asset: &Path,
    use_json: bool,
    show_ik: bool,
    output: Option<&Path>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if output.is_some() && !use_json {
        return usage_error("inspect --output requires --json");
    }
    if use_json && show_ik {
        return usage_error("inspect --json and --ik cannot be combined");
    }
    if show_ik {
        if detect_path_format(asset)? != mmd_anim_format::MmdFormatKind::Pmx {
            return usage_error("inspect --ik requires a PMX model file");
        }
        return commands::import::import_pmx_ik_summary(asset);
    }
    if use_json {
        if let Some(output) = output {
            return commands::parse::parse_format_json_to_file(asset, output);
        }
        return commands::parse::parse_format_json(asset);
    }
    if detect_path_format(asset)? == mmd_anim_format::MmdFormatKind::Pmx {
        commands::parse::parse_pmx_summary(asset)
    } else {
        commands::parse::parse_format_summary(asset)
    }
}

fn dispatch_import(
    model: &Path,
    motion: Option<&Path>,
    use_json: bool,
    show_clip: bool,
    frame: Option<f32>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if use_json {
        return usage_error("import --json is not supported by the existing import summaries");
    }
    if show_clip && frame.is_some() {
        return usage_error("import --clip and --frame cannot be combined");
    }

    if let Some(motion) = motion {
        if show_clip {
            return commands::import::import_pair_clip_summary(model, motion);
        }
        if let Some(frame) = frame {
            return commands::import::import_pair_frame_summary(model, motion, frame);
        }
        return commands::import::import_pair_summary(model, motion);
    }

    if show_clip || frame.is_some() {
        return usage_error("import --clip and --frame require a motion argument");
    }
    match detect_path_format(model)? {
        mmd_anim_format::MmdFormatKind::Pmx => commands::import::import_pmx_summary(model),
        mmd_anim_format::MmdFormatKind::Pmd => commands::import::import_pmd_summary(model),
        mmd_anim_format::MmdFormatKind::Vmd => commands::import::import_vmd_summary(model),
        _ => usage_error(format!(
            "unsupported or unrecognized file format: {}; import requires a PMX, PMD, or VMD input when no motion is provided",
            model.display()
        )),
    }
}

fn dispatch_roundtrip(
    asset: &Path,
    use_json: bool,
    via_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match (via_json, use_json) {
        (true, true) => commands::export::export_json_roundtrip_json(asset),
        (true, false) => commands::export::export_json_roundtrip_summary(asset),
        (false, true) => commands::export::export_roundtrip_json(asset),
        (false, false) => commands::export::export_roundtrip_summary(asset),
    }
}

fn dispatch_bench(
    model: Option<PathBuf>,
    motion: Option<PathBuf>,
    synthetic: bool,
    extra_args: Vec<String>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if synthetic {
        let mut raw = Vec::<String>::new();
        if let Some(model) = model {
            raw.push(model.to_string_lossy().into_owned());
        }
        if let Some(motion) = motion {
            raw.push(motion.to_string_lossy().into_owned());
        }
        raw.extend(extra_args);
        let mut iter = raw.into_iter();
        commands::bench::parse_bench_synthetic_args(&mut iter)
            .and_then(commands::bench::bench_synthetic)
    } else {
        let Some(model) = model else {
            return usage_error("bench requires <model> <motion> unless --synthetic is set");
        };
        let Some(motion) = motion else {
            return usage_error("bench requires <model> <motion> unless --synthetic is set");
        };
        let mut raw = vec![
            model.to_string_lossy().into_owned(),
            motion.to_string_lossy().into_owned(),
        ];
        raw.extend(extra_args);
        let mut iter = raw.into_iter();
        commands::bench::parse_bench_pair_args(&mut iter).and_then(commands::bench::bench_pair)
    }
}

fn dispatch_verify(
    target: &Path,
    mode: Option<VerifyMode>,
    diagnose: Option<Vec<String>>,
    compare: bool,
    use_json: bool,
    eval_frame: Option<f32>,
    sample_frame_offset: Option<f32>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let Some(mode) = mode else {
        if diagnose.is_some()
            || compare
            || use_json
            || eval_frame.is_some()
            || sample_frame_offset.is_some()
        {
            return usage_error("verify without --mode only supports oracle summary files");
        }
        return commands::oracle::oracle_summary(&target.to_string_lossy());
    };

    match mode {
        VerifyMode::Numeric | VerifyMode::Camera => {
            if compare || use_json {
                return usage_error(
                    "verify --mode numeric|camera does not support --compare or --json",
                );
            }
            if sample_frame_offset.is_some() {
                return usage_error(
                    "verify --mode numeric|camera does not support --sample-frame-offset",
                );
            }
            if let Some(parts) = diagnose {
                return dispatch_numeric_diagnose(target, parts, eval_frame);
            }
            if eval_frame.is_some() {
                return usage_error("verify --eval-frame requires --diagnose");
            }
            commands::compare::compare_numeric_manifest(target)
        }
        VerifyMode::Ik => {
            dispatch_verify_ik(target, diagnose, compare, use_json, sample_frame_offset)
        }
        VerifyMode::Parser => {
            if diagnose.is_some()
                || compare
                || use_json
                || eval_frame.is_some()
                || sample_frame_offset.is_some()
            {
                return usage_error(
                    "verify --mode parser only supports parser golden summary for the target root",
                );
            }
            golden_parser_summary(target)
        }
    }
}

fn dispatch_numeric_diagnose(
    manifest: &Path,
    parts: Vec<String>,
    eval_frame: Option<f32>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut parts = parts.into_iter();
    let case_name = parts.next().ok_or("missing diagnose case name")?;
    let frame_text = parts.next().ok_or("missing diagnose frame")?;
    let frame = frame_text
        .parse::<f32>()
        .map_err(|_| format!("invalid diagnose frame: {frame_text}"))?;
    let mut rest = Vec::new();
    if let Some(bone_name) = parts.next() {
        rest.push(bone_name);
    }
    if let Some(eval_frame) = eval_frame {
        rest.push("--eval-frame".to_owned());
        rest.push(eval_frame.to_string());
    }

    let diagnose_options = commands::compare::parse_diagnose_numeric_bone_rest(rest, frame);
    let eval_frame = diagnose_options.eval_frame;
    let bone_names = diagnose_options.bone_names;
    if bone_names.is_empty() {
        eprintln!("{}", commands::compare::DIAGNOSE_NUMERIC_BONE_USAGE);
        return Ok(ExitCode::from(2));
    }
    commands::compare::diagnose_numeric_bones(manifest, &case_name, frame, eval_frame, &bone_names)
}

fn dispatch_verify_ik(
    root: &Path,
    diagnose: Option<Vec<String>>,
    compare: bool,
    use_json: bool,
    sample_frame_offset: Option<f32>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if let Some(parts) = diagnose {
        if compare || use_json {
            return usage_error(
                "verify --mode ik --diagnose cannot be combined with --compare or --json",
            );
        }
        return dispatch_ik_diagnose(root, parts, sample_frame_offset);
    }

    if compare || use_json || sample_frame_offset.is_some() {
        let mut raw = vec![root.to_string_lossy().into_owned()];
        if let Some(offset) = sample_frame_offset {
            raw.push(offset.to_string());
        }
        if use_json {
            raw.push("--json".to_owned());
        }
        let mut iter = raw.into_iter();
        return match commands::golden::parse_golden_ik_compare_args(&mut iter) {
            Ok((root, offset, use_json)) => {
                commands::golden::golden_ik_compare(Path::new(&root), offset, use_json)
            }
            Err(error) => {
                eprintln!("{error}");
                Ok(ExitCode::from(2))
            }
        };
    }

    golden_ik_summary(root)
}

fn dispatch_ik_diagnose(
    root: &Path,
    parts: Vec<String>,
    sample_frame_offset: Option<f32>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut parts = parts.into_iter();
    let case_name = parts.next().ok_or("missing diagnose case name")?;
    let frame_text = parts.next().ok_or("missing diagnose frame")?;
    let frame = frame_text
        .parse::<i32>()
        .map_err(|_| format!("invalid IK diagnose frame: {frame_text}"))?;
    let bone_name = parts
        .next()
        .ok_or("verify --mode ik --diagnose requires a bone name")?;
    let offset = sample_frame_offset.unwrap_or(0.0);

    commands::golden::golden_ik_diagnose(root, &case_name, frame, &bone_name, offset)
}

struct PmmFrameRangeArgs {
    current_frame: Option<i32>,
    current_frame_text: Option<i32>,
    begin_frame: Option<i32>,
    end_frame: Option<i32>,
    begin_frame_enabled: Option<String>,
    end_frame_enabled: Option<String>,
}

impl PmmFrameRangeArgs {
    fn has_any(&self) -> bool {
        self.current_frame.is_some()
            || self.current_frame_text.is_some()
            || self.begin_frame.is_some()
            || self.end_frame.is_some()
            || self.begin_frame_enabled.is_some()
            || self.end_frame_enabled.is_some()
    }

    fn to_option_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(value) = self.current_frame {
            args.push("--current-frame".to_owned());
            args.push(value.to_string());
        }
        if let Some(value) = self.current_frame_text {
            args.push("--current-frame-text".to_owned());
            args.push(value.to_string());
        }
        if let Some(value) = self.begin_frame {
            args.push("--begin-frame".to_owned());
            args.push(value.to_string());
        }
        if let Some(value) = self.end_frame {
            args.push("--end-frame".to_owned());
            args.push(value.to_string());
        }
        if let Some(value) = &self.begin_frame_enabled {
            args.push("--begin-frame-enabled".to_owned());
            args.push(value.clone());
        }
        if let Some(value) = &self.end_frame_enabled {
            args.push("--end-frame-enabled".to_owned());
            args.push(value.clone());
        }
        args
    }
}

fn dispatch_patch(
    input: &Path,
    model_path: Option<Vec<String>>,
    frame_range: Option<PathBuf>,
    frame_args: PmmFrameRangeArgs,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match (model_path, frame_range) {
        (Some(values), None) => {
            if frame_args.has_any() {
                return usage_error("patch --model-path does not accept trailing options");
            }
            let [index, path, output]: [String; 3] = values
                .try_into()
                .map_err(|_| "patch --model-path requires <idx> <path> <out>")?;
            commands::patch::patch_pmm_document_model_path(
                input,
                &index,
                &path,
                &PathBuf::from(output),
            )
        }
        (None, Some(output)) => {
            if !frame_args.has_any() {
                return usage_error("patch --frame-range requires at least one frame range option");
            }
            commands::patch::patch_pmm_scene_frame_range(
                input,
                &output,
                &frame_args.to_option_args(),
            )
        }
        (Some(_), Some(_)) => {
            usage_error("patch --model-path and --frame-range cannot be combined")
        }
        (None, None) => usage_error("patch requires --model-path or --frame-range"),
    }
}

fn dispatch_export(
    input: &Path,
    output: &Path,
    from_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if from_json {
        commands::export::export_json_format(input, output)
    } else {
        commands::export::export_format(input, output)
    }
}

const FORMAT_SNIFF_BYTES: usize = 64;

fn detect_path_format(
    path: &Path,
) -> Result<mmd_anim_format::MmdFormatKind, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(path).map_err(|error| {
        format!(
            "failed to read {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
    })?;
    let mut data = [0u8; FORMAT_SNIFF_BYTES];
    let len = file.read(&mut data).map_err(|error| {
        format!(
            "failed to read {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
    })?;
    Ok(mmd_anim_format::detect_mmd_format(
        &data[..len],
        path.file_name().and_then(|v| v.to_str()),
    ))
}

fn usage_error(message: impl AsRef<str>) -> Result<ExitCode, Box<dyn std::error::Error>> {
    eprintln!("{}", message.as_ref());
    Ok(ExitCode::from(2))
}

pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    fs::read(path).map_err(|error| {
        format!(
            "failed to read {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
        .into()
    })
}

pub(crate) fn read_text_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
        .into()
    })
}

pub(crate) fn write_file(
    path: &Path,
    data: impl AsRef<[u8]>,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, data).map_err(|error| {
        format!(
            "failed to write {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
        .into()
    })
}

pub(crate) fn diagnostics_suffix(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!(" diagnostics={count}")
    }
}

pub(crate) fn unsupported_format_error(path: &Path) -> Box<dyn std::error::Error> {
    format!(
        "unsupported or unrecognized file format: {}",
        path.display()
    )
    .into()
}

fn format_cli_error(error: &(dyn std::error::Error + 'static)) -> String {
    if let Some(io_error) = error.downcast_ref::<io::Error>() {
        return format!("I/O error: {}", io_error_label(io_error.kind()));
    }
    error.to_string()
}

fn io_error_label(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "file not found",
        io::ErrorKind::PermissionDenied => "permission denied",
        io::ErrorKind::InvalidData => "invalid data",
        io::ErrorKind::UnexpectedEof => "unexpected end of file",
        io::ErrorKind::AlreadyExists => "already exists",
        io::ErrorKind::WouldBlock => "operation would block",
        io::ErrorKind::TimedOut => "operation timed out",
        io::ErrorKind::Interrupted => "operation interrupted",
        _ => "I/O error",
    }
}

// ---------------------------------------------------------------------------
// Functions that remain in the crate root (used by multiple modules)
// ---------------------------------------------------------------------------

pub(crate) fn resolve_maybe_absolute(root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

pub(crate) fn translation_checksum(matrices: &[glam::Mat4]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for m in matrices {
        hash ^= m.w_axis.x.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= m.w_axis.y.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= m.w_axis.z.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

pub(crate) fn f32_checksum(values: &[f32]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for value in values {
        hash ^= value.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

pub(crate) fn copy_world_matrices_to_f32(matrices: &[glam::Mat4], out: &mut [f32]) {
    debug_assert!(out.len() >= matrices.len() * 16);
    for (index, matrix) in matrices.iter().enumerate() {
        let offset = index * 16;
        out[offset..offset + 16].copy_from_slice(&matrix.to_cols_array());
    }
}

// ---------------------------------------------------------------------------
// Functions that stay in main because they reference main-only schemas
// ---------------------------------------------------------------------------

fn golden_ik_summary(root: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use crate::schema::{
        DEFAULT_FOCUSED_IK_BONE_NAMES, GoldenIkBatchManifest, GoldenIkFixture, MmdDumperOracleDump,
    };

    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&read_text_file(&manifest_path)?)?;
    let mut parsed_cases = 0usize;
    let mut parsed_frames = 0usize;
    let mut parsed_bones = 0usize;
    let mut focused_frame_hits = 0usize;
    let mut missing = Vec::new();

    for case in &manifest.cases {
        let case_root = root.join(&case.name);
        let fixture_path = case_root.join("fixture.json");
        if !fixture_path.exists() {
            missing.push(fixture_path);
            continue;
        }

        let fixture = GoldenIkFixture::from_json_str(&read_text_file(&fixture_path)?)?;
        let oracle_path = resolve_maybe_absolute(&case_root, &fixture.output);
        if !oracle_path.exists() {
            missing.push(oracle_path);
            continue;
        }

        let frames = if fixture.frames.is_empty() {
            case.frames.as_slice()
        } else {
            fixture.frames.as_slice()
        };
        let dump =
            MmdDumperOracleDump::from_jsonl_str(&read_text_file(&oracle_path)?, Some(frames))?;
        parsed_cases += 1;
        parsed_frames += dump.frames.len();
        parsed_bones += dump
            .frames
            .first()
            .and_then(|frame| frame.models.first())
            .map(|model| model.bones.len())
            .unwrap_or(0);
        for frame in &dump.frames {
            let focused_count = frame
                .models
                .first()
                .map(|model| {
                    model
                        .focused_ik_bones(DEFAULT_FOCUSED_IK_BONE_NAMES)
                        .count()
                })
                .unwrap_or(0);
            if focused_count == 0 {
                return Err(format!(
                    "{} frame={} has no focused IK bones",
                    case.name, frame.frame
                )
                .into());
            }
            focused_frame_hits += 1;
        }
    }

    if !missing.is_empty() {
        for path in missing {
            eprintln!("missing: {}", path.display());
        }
        return Err("one or more golden IK oracle files are missing".into());
    }

    println!(
        "MMDDumper golden IK: cases={} selectedFrames={} firstFrameBoneTotal={} focusedFrameHits={}",
        parsed_cases, parsed_frames, parsed_bones, focused_frame_hits
    );
    Ok(ExitCode::SUCCESS)
}

fn golden_parser_summary(root: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use crate::schema::{GoldenIkBatchManifest, GoldenIkFixture, MmdDumperOracleDump};

    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&read_text_file(&manifest_path)?)?;
    let mut parsed_cases = 0usize;
    let mut skipped_unsupported = 0usize;
    let mut missing_files = Vec::new();
    let mut matched_bones = 0usize;
    let mut missing_bones = 0usize;
    let mut matched_morphs = 0usize;
    let mut missing_morphs = 0usize;

    for case in &manifest.cases {
        let pmx_path = PathBuf::from(&case.pmx);
        if pmx_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("pmx"))
        {
            skipped_unsupported += 1;
            continue;
        }
        if !pmx_path.exists() {
            missing_files.push(pmx_path);
            continue;
        }

        let case_root = root.join(&case.name);
        let fixture_path = case_root.join("fixture.json");
        if !fixture_path.exists() {
            missing_files.push(fixture_path);
            continue;
        }
        let fixture = GoldenIkFixture::from_json_str(&read_text_file(&fixture_path)?)?;
        let oracle_path = resolve_maybe_absolute(&case_root, &fixture.output);
        if !oracle_path.exists() {
            missing_files.push(oracle_path);
            continue;
        }

        let parsed = mmd_anim_format::parse_pmx_model(&read_file(&pmx_path)?)?;
        let bone_names = parsed
            .skeleton
            .bones
            .iter()
            .map(|bone| bone.name.as_str())
            .collect::<HashSet<_>>();
        let morph_names = parsed
            .morphs
            .iter()
            .map(|morph| morph.name.as_str())
            .collect::<HashSet<_>>();

        let frames = if fixture.frames.is_empty() {
            case.frames.as_slice()
        } else {
            fixture.frames.as_slice()
        };
        let dump =
            MmdDumperOracleDump::from_jsonl_str(&read_text_file(&oracle_path)?, Some(frames))?;
        parsed_cases += 1;

        let Some(model) = dump.frames.first().and_then(|frame| frame.models.first()) else {
            continue;
        };
        for bone in &model.bones {
            if bone_names.contains(bone.name.as_str()) {
                matched_bones += 1;
            } else {
                missing_bones += 1;
            }
        }
        for morph in &model.morphs {
            if morph_names.contains(morph.name.as_str()) {
                matched_morphs += 1;
            } else {
                missing_morphs += 1;
            }
        }
    }

    if !missing_files.is_empty() {
        for path in missing_files {
            eprintln!("missing: {}", path.display());
        }
        return Err("one or more Golden parser files are missing".into());
    }

    println!(
        "MMDDumper parser golden: cases={} skippedUnsupported={} matchedBones={} missingBones={} matchedMorphs={} missingMorphs={}",
        parsed_cases,
        skipped_unsupported,
        matched_bones,
        missing_bones,
        matched_morphs,
        missing_morphs
    );
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use glam::{Quat, Vec3A};
    use mmd_anim_runtime::{
        AnimationClip, BoneAnimationBinding, BoneIndex, BoneInit, ModelArena, MovableBoneKeyframe,
        MovableBoneTrack, RuntimeInstance,
    };

    use crate::commands::{bench, compare, export, patch};

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("mmd-anim-cli-{name}-{nanos}"))
    }

    #[test]
    fn test_synthetic_model_bone_count() {
        let bones = (0..8)
            .map(|i| {
                let parent = if i == 0 {
                    None
                } else {
                    Some(BoneIndex(i as u32 - 1))
                };
                BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0))
            })
            .collect();
        let model = ModelArena::new(bones).unwrap();
        assert_eq!(model.bone_count(), 8);
    }

    #[test]
    fn test_synthetic_clip_track_count() {
        let tracks: Vec<_> = (0..4)
            .map(|i| {
                let track = MovableBoneTrack::from_keyframes(vec![
                    MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                    MovableBoneKeyframe::new(
                        30,
                        Vec3A::new(1.0, 0.0, 0.0),
                        Quat::from_axis_angle(Vec3A::Y.into(), 0.5),
                    ),
                ]);
                BoneAnimationBinding {
                    bone: BoneIndex(i as u32),
                    track,
                }
            })
            .collect();
        let clip = AnimationClip::new(tracks);
        assert_eq!(clip.bone_track_count(), 4);
    }

    #[test]
    fn test_bench_checksum_deterministic() {
        let bones = (0..4)
            .map(|i| {
                let parent = if i == 0 {
                    None
                } else {
                    Some(BoneIndex(i as u32 - 1))
                };
                BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0))
            })
            .collect();
        let model = Arc::new(ModelArena::new(bones).unwrap());
        let tracks: Vec<_> = (0..4)
            .map(|i| {
                let track = MovableBoneTrack::from_keyframes(vec![
                    MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                    MovableBoneKeyframe::new(
                        30,
                        Vec3A::new(1.0, 0.0, 0.0),
                        Quat::from_axis_angle(Vec3A::Y.into(), 0.5),
                    ),
                ]);
                BoneAnimationBinding {
                    bone: BoneIndex(i as u32),
                    track,
                }
            })
            .collect();
        let clip = AnimationClip::new(tracks);

        let mut r1 = RuntimeInstance::new(Arc::clone(&model));
        let mut r2 = RuntimeInstance::new(model);
        r1.evaluate_clip_frame(&clip, 15.0);
        r2.evaluate_clip_frame(&clip, 15.0);
        assert_eq!(
            translation_checksum(r1.world_matrices()),
            translation_checksum(r2.world_matrices()),
        );
    }

    #[test]
    fn bench_synthetic_args_use_defaults() {
        let mut args = Vec::<String>::new().into_iter();
        let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 1);
        assert_eq!(cfg.bones, 32);
        assert_eq!(cfg.frames, 1000);
        assert!(!cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_json_flag() {
        let mut args = vec!["--json".to_owned()].into_iter();
        let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 1);
        assert_eq!(cfg.bones, 32);
        assert_eq!(cfg.frames, 1000);
        assert!(cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_json_with_positional() {
        let mut args = vec![
            "4".to_owned(),
            "--json".to_owned(),
            "16".to_owned(),
            "50".to_owned(),
        ]
        .into_iter();
        let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 4);
        assert_eq!(cfg.bones, 16);
        assert_eq!(cfg.frames, 50);
        assert!(cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_json_after_positional() {
        let mut args = vec![
            "2".to_owned(),
            "8".to_owned(),
            "200".to_owned(),
            "--json".to_owned(),
        ]
        .into_iter();
        let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 2);
        assert_eq!(cfg.bones, 8);
        assert_eq!(cfg.frames, 200);
        assert!(cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_reject_unknown_flag() {
        let mut args = vec!["--unknown".to_owned()].into_iter();
        let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("unknown flag"));
    }

    #[test]
    fn bench_synthetic_args_reject_invalid_models() {
        let mut args = vec!["nope".to_owned()].into_iter();
        let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("invalid models"));
    }

    #[test]
    fn bench_synthetic_args_reject_zero_models() {
        let mut args = vec!["0".to_owned()].into_iter();
        let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("models must be positive"));
    }

    #[test]
    fn bench_synthetic_args_reject_extra_values() {
        let mut args = vec![
            "1".to_owned(),
            "8".to_owned(),
            "100".to_owned(),
            "extra".to_owned(),
        ]
        .into_iter();
        let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("unexpected extra argument"));
    }

    #[test]
    fn compare_numeric_mixed_manifest_dispatches_by_case_kind() {
        let temp = unique_test_dir("compare-numeric-mixed");
        fs::create_dir_all(&temp).unwrap();
        fs::write(
            temp.join("camera.vmd"),
            include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd"),
        )
        .unwrap();
        fs::write(
            temp.join("camera-oracle.json"),
            r#"{
                "frames": [
                    {
                        "frame": 0,
                        "camera": {
                            "distance": -30.5,
                            "position": [1.0, 2.0, 3.0],
                            "rotation": [0.1, -0.2, 0.3],
                            "fov": 35,
                            "perspective": true
                        }
                    }
                ]
            }"#,
        )
        .unwrap();
        fs::write(
            temp.join("manifest.json"),
            r#"{
                "cases": [
                    {
                        "name": "camera",
                        "kind": "camera-vmd",
                        "assets": { "cameraMotion": "camera.vmd" },
                        "oracle": { "path": "camera-oracle.json" },
                        "compare": { "epsilon": 0.003 }
                    },
                    {
                        "name": "motion",
                        "kind": "motion-numeric",
                        "assets": {
                            "model": "missing.pmx",
                            "motion": "missing.vmd"
                        },
                        "oracle": { "path": "missing.json" },
                        "frames": [0],
                        "compare": { "targets": ["bones"], "epsilon": 0.003 }
                    }
                ]
            }"#,
        )
        .unwrap();

        let error = compare::compare_numeric_manifest(&temp.join("manifest.json")).unwrap_err();
        let error = error.to_string();
        assert!(error.contains("cameraMismatches=0"));
        assert!(error.contains("motionMissing=1"));
        assert!(!error.contains("unsupported kind"));

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn numeric_compare_failure_count_includes_motion_mismatches() {
        let camera = compare::CameraNumericCompareStats::default();
        let motion = compare::MotionNumericCompareStats {
            mismatch_count: 1,
            ..compare::MotionNumericCompareStats::default()
        };

        assert_eq!(compare::numeric_compare_failure_count(&camera, &motion), 1);
    }

    #[test]
    fn motion_case_focus_bones_prefers_case_metadata_focus() {
        let case = serde_json::json!({
            "metadata": {
                "focus": {
                    "bones": ["右袖", "左袖"]
                }
            }
        });
        let defaults = vec!["左ひざ".to_owned()];

        assert_eq!(
            compare::motion_case_focus_bones(&case, Some(&defaults)),
            vec!["右袖".to_owned(), "左袖".to_owned()]
        );
    }

    #[test]
    fn motion_case_focus_bones_uses_default_focus() {
        let case = serde_json::json!({});
        let defaults = vec!["右腕".to_owned(), "左腕".to_owned()];

        assert_eq!(
            compare::motion_case_focus_bones(&case, Some(&defaults)),
            defaults
        );
    }

    #[test]
    fn json_f32_reads_nested_number() {
        let value = serde_json::json!({
            "compare": {
                "evalFrameOffset": 1.25
            }
        });

        assert_eq!(
            compare::json_f32(&value, "/compare/evalFrameOffset"),
            Some(1.25)
        );
    }

    #[test]
    fn vmd_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::VmdParsedAnimation {
            kind: "vmd",
            metadata: mmd_anim_format::vmd::VmdParsedMetadata {
                format: "vmd",
                model_name: "miku".to_owned(),
                model_name_bytes: Vec::new(),
                counts: mmd_anim_format::vmd::VmdParsedCounts {
                    bones: 1,
                    morphs: 2,
                    cameras: 3,
                    lights: 4,
                    self_shadows: 5,
                    properties: 6,
                },
                max_frame: 120,
            },
            bone_frames: Vec::new(),
            morph_frames: Vec::new(),
            camera_frames: Vec::new(),
            light_frames: Vec::new(),
            self_shadow_frames: Vec::new(),
            property_frames: Vec::new(),
        };
        let value = export::vmd_roundtrip_json(
            Path::new("motion.vmd"),
            "parse-json-export-parse",
            10,
            20,
            Some(30),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "vmd");
        assert_eq!(value["mode"], "parse-json-export-parse");
        assert_eq!(value["bytesIn"], 10);
        assert_eq!(value["bytesOut"], 20);
        assert_eq!(value["jsonBytes"], 30);
        assert_eq!(value["counts"]["boneFrames"], 1);
        assert_eq!(value["counts"]["propertyFrames"], 6);
        assert_eq!(value["maxFrame"], 120);
    }

    #[test]
    fn vpd_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::VpdParsedPose {
            format: "vpd",
            model_file: "model.pmx".to_owned(),
            bone_count: 2,
            bones: Vec::new(),
            diagnostics: Vec::new(),
        };
        let value = export::vpd_roundtrip_json(
            Path::new("pose.vpd"),
            "parse-export-parse",
            11,
            22,
            None,
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "vpd");
        assert_eq!(value["mode"], "parse-export-parse");
        assert_eq!(value["bytesIn"], 11);
        assert_eq!(value["bytesOut"], 22);
        assert!(value["jsonBytes"].is_null());
        assert_eq!(value["counts"]["bones"], 2);
    }

    #[test]
    fn accessory_roundtrip_json_reports_text_mesh_material_export_scope() {
        let parsed = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 100,
            text: true,
            header: "xof 0303txt 0032".to_owned(),
            mesh_count: 1,
            material_count: 1,
            mesh_summaries: vec![mmd_anim_format::xfile::AccessoryMeshSummary {
                vertex_count: 3,
                face_count: 1,
                positions: vec![[0.0, 0.0, 0.0]],
                face_indices: vec![vec![0, 1, 2]],
                normals: Vec::new(),
                normal_face_indices: Vec::new(),
                texture_coordinates: vec![[0.0, 0.0]],
                vertex_colors: vec![mmd_anim_format::xfile::AccessoryVertexColor {
                    vertex_index: 2,
                    color: [1.0, 0.5, 0.25, 1.0],
                }],
                material_indices: vec![0],
                material_start_index: 0,
                material_count: 1,
            }],
            materials: vec![mmd_anim_format::xfile::AccessoryMaterial {
                name: Some("mat".to_owned()),
                face_color: Some([1.0, 1.0, 1.0, 1.0]),
                power: Some(5.0),
                specular_color: Some([0.0, 0.0, 0.0]),
                emissive_color: Some([0.0, 0.0, 0.0]),
                texture_references: vec!["tex.png".to_owned()],
            }],
            vac_settings: None,
            texture_references: vec!["tex.png".to_owned()],
            diagnostics: Vec::new(),
        };
        let value = export::accessory_roundtrip_json(
            Path::new("stage.x"),
            "parse-json-export-parse",
            100,
            50,
            Some(200),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "x");
        assert_eq!(value["counts"]["meshes"], 1);
        assert_eq!(value["counts"]["materials"], 1);
        assert_eq!(value["counts"]["meshVertices"], 3);
        assert_eq!(value["counts"]["meshFaces"], 1);
        assert_eq!(value["counts"]["meshNormals"], 0);
        assert_eq!(value["counts"]["meshTextureCoordinates"], 1);
        assert_eq!(value["counts"]["meshVertexColors"], 1);
        assert_eq!(value["counts"]["meshMaterialIndices"], 1);
        assert_eq!(
            value["metadata"]["exportScope"],
            "text-mesh-material-attributes"
        );
        assert_eq!(value["metadata"]["meshMaterialReemitted"], true);
        assert_eq!(
            value["metadata"]["preservedFields"],
            serde_json::json!([
                "format",
                "header",
                "textureReferences",
                "meshSummaries",
                "materials"
            ])
        );
    }

    #[test]
    fn ensure_accessory_roundtrip_rejects_text_flag_changes() {
        let expected = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 16,
            text: false,
            header: "xof 0303bin 0032".to_owned(),
            mesh_count: 0,
            material_count: 0,
            mesh_summaries: Vec::new(),
            materials: Vec::new(),
            vac_settings: None,
            texture_references: Vec::new(),
            diagnostics: Vec::new(),
        };
        let mut actual = expected.clone();
        actual.text = true;

        let error = export::ensure_accessory_roundtrip(&expected, &actual).unwrap_err();
        assert!(error.contains("text flag changed"));
    }

    #[test]
    fn ensure_accessory_roundtrip_accepts_multi_mesh_material_ownership() {
        let expected = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 100,
            text: true,
            header: "xof 0303txt 0032".to_owned(),
            mesh_count: 2,
            material_count: 2,
            mesh_summaries: vec![
                mmd_anim_format::xfile::AccessoryMeshSummary {
                    vertex_count: 3,
                    face_count: 1,
                    positions: vec![[0.0, 0.0, 0.0]],
                    face_indices: vec![vec![0, 1, 2]],
                    normals: Vec::new(),
                    normal_face_indices: Vec::new(),
                    texture_coordinates: Vec::new(),
                    vertex_colors: Vec::new(),
                    material_indices: vec![0],
                    material_start_index: 0,
                    material_count: 1,
                },
                mmd_anim_format::xfile::AccessoryMeshSummary {
                    vertex_count: 3,
                    face_count: 1,
                    positions: vec![[0.0, 0.0, 1.0]],
                    face_indices: vec![vec![0, 2, 1]],
                    normals: Vec::new(),
                    normal_face_indices: Vec::new(),
                    texture_coordinates: Vec::new(),
                    vertex_colors: Vec::new(),
                    material_indices: vec![0],
                    material_start_index: 1,
                    material_count: 1,
                },
            ],
            materials: vec![
                mmd_anim_format::xfile::AccessoryMaterial {
                    name: Some("mat0".to_owned()),
                    face_color: Some([1.0, 1.0, 1.0, 1.0]),
                    power: Some(5.0),
                    specular_color: Some([0.0, 0.0, 0.0]),
                    emissive_color: Some([0.0, 0.0, 0.0]),
                    texture_references: Vec::new(),
                },
                mmd_anim_format::xfile::AccessoryMaterial {
                    name: Some("mat1".to_owned()),
                    face_color: Some([0.5, 0.5, 0.5, 1.0]),
                    power: Some(2.0),
                    specular_color: Some([0.0, 0.0, 0.0]),
                    emissive_color: Some([0.0, 0.0, 0.0]),
                    texture_references: Vec::new(),
                },
            ],
            vac_settings: None,
            texture_references: Vec::new(),
            diagnostics: Vec::new(),
        };
        let actual = expected.clone();

        export::ensure_accessory_roundtrip(&expected, &actual).unwrap();
    }

    #[test]
    fn ensure_accessory_json_roundtrip_rejects_dto_changes() {
        let expected = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 16,
            text: true,
            header: "xof 0303txt 0032".to_owned(),
            mesh_count: 0,
            material_count: 0,
            mesh_summaries: Vec::new(),
            materials: Vec::new(),
            vac_settings: None,
            texture_references: vec!["tex.png".to_owned()],
            diagnostics: Vec::new(),
        };
        let mut actual = expected.clone();
        actual.texture_references.clear();

        let error = export::ensure_accessory_json_roundtrip(&expected, &actual).unwrap_err();
        assert_eq!(error, "Accessory JSON data differs after re-encoding");
    }

    #[test]
    fn pmd_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::PmdParsedModel {
            metadata: mmd_anim_format::pmd::PmdParsedMetadata {
                format: "pmd".to_owned(),
                version: 1.0,
                encoding: "shift-jis".to_owned(),
                name: "model".to_owned(),
                name_bytes: Vec::new(),
                english_name: String::new(),
                english_name_bytes: Vec::new(),
                comment: String::new(),
                comment_bytes: Vec::new(),
                english_comment: String::new(),
                english_comment_bytes: Vec::new(),
                counts: mmd_anim_format::pmd::PmdParsedCounts {
                    vertices: 1,
                    faces: 2,
                    materials: 3,
                    bones: 4,
                    ik: 5,
                    morphs: 6,
                    display_frames: 7,
                    rigid_bodies: 8,
                    joints: 9,
                },
            },
            geometry: mmd_anim_format::pmd::PmdParsedGeometry {
                vertices: Vec::new(),
                indices: Vec::new(),
            },
            materials: Vec::new(),
            toon_textures: Vec::new(),
            toon_texture_bytes: Vec::new(),
            skeleton: mmd_anim_format::pmd::PmdParsedSkeleton {
                bones: Vec::new(),
                ik: Vec::new(),
            },
            morphs: Vec::new(),
            display_frames: Vec::new(),
            rigid_bodies: Vec::new(),
            joints: Vec::new(),
            diagnostics: Vec::new(),
        };
        let value = export::pmd_roundtrip_json(
            Path::new("model.pmd"),
            "parse-json-export-parse",
            10,
            20,
            Some(30),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "pmd");
        assert_eq!(value["mode"], "parse-json-export-parse");
        assert_eq!(value["bytesIn"], 10);
        assert_eq!(value["bytesOut"], 20);
        assert_eq!(value["jsonBytes"], 30);
        assert_eq!(value["counts"]["vertices"], 1);
        assert_eq!(value["counts"]["ik"], 5);
        assert_eq!(value["counts"]["joints"], 9);
    }

    #[test]
    fn pmx_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::PmxParsedModel {
            metadata: mmd_anim_format::pmx::PmxParsedMetadata {
                format: "pmx".to_owned(),
                version: 2.0,
                encoding: "utf-8".to_owned(),
                name: "model".to_owned(),
                english_name: String::new(),
                comment: String::new(),
                english_comment: String::new(),
                counts: mmd_anim_format::pmx::PmxParsedCounts {
                    vertices: 1,
                    faces: 2,
                    materials: 3,
                    bones: 4,
                    morphs: 5,
                    display_frames: 6,
                    rigid_bodies: 7,
                    joints: 8,
                    soft_bodies: 9,
                },
                index_sizes: mmd_anim_format::pmx::PmxParsedIndexSizes {
                    vertex: 4,
                    texture: 1,
                    material: 1,
                    bone: 2,
                    morph: 1,
                    rigid_body: 1,
                },
                additional_uv_count: 0,
            },
            geometry: mmd_anim_format::pmx::PmxParsedGeometry {
                positions: Vec::new(),
                normals: Vec::new(),
                uvs: Vec::new(),
                additional_uvs: Vec::new(),
                indices: Vec::new(),
                skin_indices: Vec::new(),
                skin_weights: Vec::new(),
                edge_scale: Vec::new(),
                material_groups: Vec::new(),
                sdef: mmd_anim_format::pmx::PmxParsedSdef::default(),
                qdef: mmd_anim_format::pmx::PmxParsedQdef::default(),
            },
            materials: Vec::new(),
            skeleton: mmd_anim_format::pmx::PmxParsedSkeleton { bones: Vec::new() },
            morphs: Vec::new(),
            display_frames: Vec::new(),
            rigid_bodies: Vec::new(),
            joints: Vec::new(),
            soft_bodies: Vec::new(),
            diagnostics: Vec::new(),
        };
        let value = export::pmx_roundtrip_json(
            Path::new("model.pmx"),
            "parse-json-export-parse",
            10,
            20,
            Some(30),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "pmx");
        assert_eq!(value["mode"], "parse-json-export-parse");
        assert_eq!(value["bytesIn"], 10);
        assert_eq!(value["bytesOut"], 20);
        assert_eq!(value["jsonBytes"], 30);
        assert_eq!(value["metadata"]["version"], 2.0);
        assert_eq!(value["metadata"]["encoding"], "utf-8");
        assert_eq!(value["metadata"]["additionalUvCount"], 0);
        assert_eq!(value["metadata"]["indexSizes"]["vertex"], 4);
        assert_eq!(value["metadata"]["indexSizes"]["bone"], 2);
        assert_eq!(value["counts"]["vertices"], 1);
        assert_eq!(value["counts"]["softBodies"], 9);
    }

    #[test]
    fn resolve_pmx_path_for_pmm_makes_relative_existing_path_absolute() {
        let relative = Path::new("Cargo.toml");
        assert!(
            relative.exists(),
            "Cargo.toml must exist for this repository-local test"
        );

        let resolved = export::resolve_pmx_path_for_pmm(relative)
            .expect("canonicalize must succeed for an existing repository file");

        let resolved_path = Path::new(&resolved);
        assert!(
            resolved_path.is_absolute(),
            "expected canonical PMX path for PMM to be absolute, got: {}",
            resolved
        );
        assert!(
            !resolved.starts_with(r"\\?\"),
            "expected PMM path to avoid Windows verbatim prefix for MMD GUI loading, got: {}",
            resolved
        );
    }

    #[test]
    fn export_pmm_scene_embeds_clean_absolute_model_path() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmx_path = format_crate.join("fixtures/pmx/ik_multi_axis_limit.pmx");
        let vmd_path = format_crate.join("fixtures/vmd/ik_multi_bone_nondefault.vmd");

        let model_path_text =
            export::resolve_pmx_path_for_pmm(&pmx_path).expect("PMX fixture path must resolve");
        let model_bytes = fs::read(&pmx_path).expect("PMX fixture must exist");
        let motion_bytes = fs::read(&vmd_path).expect("VMD fixture must exist");
        let model = mmd_anim_format::parse_pmx_model(&model_bytes).expect("PMX fixture parses");
        let motion =
            mmd_anim_format::parse_vmd_animation(&motion_bytes).expect("VMD fixture parses");

        let report = mmd_anim_format::export_pmm_scene_from_pmx_vmd(
            &model,
            &motion,
            &model_path_text,
            &mmd_anim_format::PmmSceneExportOptions::default(),
        );
        let reparsed =
            mmd_anim_format::parse_pmm_manifest(&report.bytes).expect("exported PMM reparses");
        let document = reparsed
            .document_summary
            .as_ref()
            .expect("exported PMM includes a document summary");
        let embedded_path = &document.models[0].path;

        assert_eq!(embedded_path, &model_path_text);
        assert!(
            Path::new(embedded_path).is_absolute(),
            "expected exported PMM model path to be absolute, got: {}",
            embedded_path
        );
        assert!(
            !embedded_path.starts_with(r"\\?\"),
            "expected exported PMM model path to avoid Windows verbatim prefix, got: {}",
            embedded_path
        );
    }

    #[test]
    fn pmm_roundtrip_json_reports_machine_readable_counts() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
        let data = fs::read(&pmm_path)
            .expect("existing PMM fixture must be readable for helper shape test");
        let parsed = mmd_anim_format::parse_pmm_manifest(&data)
            .expect("existing PMM fixture must parse for helper shape test");
        let value = export::pmm_roundtrip_json(
            Path::new("scene.pmm"),
            "parse-export-parse-lossless",
            data.len(),
            data.len(),
            true,
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "pmm");
        assert_eq!(value["mode"], "parse-export-parse-lossless");
        assert_eq!(value["bytesIn"], data.len());
        assert_eq!(value["bytesOut"], data.len());
        assert_eq!(value["version"], parsed.version);
        assert!(value["modelReferences"].is_number());
        assert!(value["assetReferences"].is_number());
        assert!(value["diagnostics"].is_number());
        assert_eq!(value["byteForByte"], true);
    }

    #[test]
    fn ensure_pmm_lossless_roundtrip_rejects_non_identical_bytes() {
        let original: &[u8] = b"Polygon Movie maker 0002\0dummy";
        let exported: &[u8] = b"different-bytes";
        let error = export::ensure_pmm_lossless_roundtrip(original, exported).unwrap_err();
        let msg = error.to_string();
        assert!(
            msg.contains("byte") || msg.contains("lossless") || msg.contains("preserve"),
            "expected rejection message about non-identical bytes, got: {}",
            msg
        );
    }

    #[test]
    fn pmm_parse_export_parse_lossless_roundtrip_via_helpers() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
        let data = fs::read(&pmm_path).expect("existing PMM fixture must be readable");
        let parsed = mmd_anim_format::parse_pmm_manifest(&data).expect("fixture parses");
        let exported = mmd_anim_format::export_pmm_manifest(&parsed);
        let reparsed = mmd_anim_format::parse_pmm_manifest(&exported).expect("exported reparses");

        export::ensure_pmm_lossless_roundtrip(&data, &exported)
            .expect("PMM parse-export-parse must be byte-for-byte lossless for parsed source");
        assert_eq!(reparsed.version, parsed.version);
        assert_eq!(
            exported, data,
            "exported bytes must equal original input bytes"
        );
    }

    #[test]
    fn export_roundtrip_summary_calls_pmm_lossless_branch_successfully() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
        let result = export::export_roundtrip_summary(&pmm_path);
        assert!(
            result.is_ok(),
            "export_roundtrip_summary on repo-local PMM fixture must succeed (lossless branch)"
        );
        let code = result.unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn export_roundtrip_json_calls_pmm_lossless_branch_successfully() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
        let result = export::export_roundtrip_json(&pmm_path);
        assert!(
            result.is_ok(),
            "export_roundtrip_json on repo-local PMM fixture must succeed (lossless branch)"
        );
        let code = result.unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn export_json_roundtrip_summary_rejects_pmm_as_unsupported() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
        let result = export::export_json_roundtrip_summary(&pmm_path);
        let err = result
            .expect_err("export_json_roundtrip_summary on PMM fixture must remain unsupported");
        let msg = err.to_string();
        assert!(
            msg.contains("not implemented") || msg.contains("PMM"),
            "expected 'not implemented' error mentioning PMM for json roundtrip, got: {}",
            msg
        );
    }

    #[test]
    fn patch_pmm_document_model_path_replaces_path_and_preserves_length() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
        let data = fs::read(&pmm_path).expect("existing PMM fixture must be readable");

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let target_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let out_dir = target_root.join("pmm-document-model-patch-test");
        let _ = fs::create_dir_all(&out_dir);
        let out_path = out_dir.join(format!("patched-doc0-{}.pmm", nanos));

        let replacement = "UserFile\\Model\\override_for_cli_patch_test.pmx";
        let result = patch::patch_pmm_document_model_path(&pmm_path, "0", replacement, &out_path);
        assert!(
            result.is_ok(),
            "patch_pmm_document_model_path on repo-local fixture must succeed: {:?}",
            result.err()
        );
        let code = result.unwrap();
        assert_eq!(code, ExitCode::SUCCESS);

        let out_data = fs::read(&out_path).expect("patched output must exist");
        assert_eq!(
            out_data.len(),
            data.len(),
            "byte length must be unchanged by document model path patch"
        );

        let reparsed =
            mmd_anim_format::parse_pmm_manifest(&out_data).expect("patched output must reparse");
        let doc = reparsed
            .document_summary
            .as_ref()
            .expect("fixture PMM must have document_summary");
        let model0 = doc
            .models
            .iter()
            .find(|m| m.document_model_index == 0)
            .expect("document model 0 must exist in fixture");
        assert_eq!(
            model0.path, replacement,
            "document model 0 path must equal replacement after patch"
        );

        let _ = fs::remove_file(&out_path);
    }

    #[test]
    fn patch_pmm_scene_frame_range_updates_fields_and_preserves_length() {
        let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
        let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
        let data = fs::read(&pmm_path).expect("existing PMM fixture must be readable");

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let target_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let out_dir = target_root.join("pmm-scene-frame-range-patch-test");
        let _ = fs::create_dir_all(&out_dir);
        let out_path = out_dir.join(format!("patched-scene-frame-range-{}.pmm", nanos));

        let options = vec![
            "--current-frame".to_string(),
            "99".to_string(),
            "--current-frame-text".to_string(),
            "77".to_string(),
            "--begin-frame-enabled".to_string(),
            "true".to_string(),
            "--end-frame-enabled".to_string(),
            "false".to_string(),
            "--begin-frame".to_string(),
            "10".to_string(),
            "--end-frame".to_string(),
            "240".to_string(),
        ];
        let result = patch::patch_pmm_scene_frame_range(&pmm_path, &out_path, &options);
        assert!(
            result.is_ok(),
            "patch_pmm_scene_frame_range on repo-local fixture must succeed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap(), ExitCode::SUCCESS);

        let out_data = fs::read(&out_path).expect("patched output must exist");
        assert_eq!(
            out_data.len(),
            data.len(),
            "byte length must be unchanged by scene frame range patch"
        );

        let reparsed =
            mmd_anim_format::parse_pmm_manifest(&out_data).expect("patched output must reparse");
        let settings = &reparsed
            .document_global_summary
            .as_ref()
            .expect("fixture PMM must have document_global_summary")
            .settings;
        assert_eq!(settings.current_frame_index, 99);
        assert_eq!(settings.current_frame_index_in_text_field, 77);
        assert!(settings.begin_frame_index_enabled);
        assert!(!settings.end_frame_index_enabled);
        assert_eq!(settings.begin_frame_index, 10);
        assert_eq!(settings.end_frame_index, 240);

        let _ = fs::remove_file(&out_path);
    }

    #[test]
    fn parse_pmm_scene_frame_range_patch_options_requires_at_least_one_option() {
        let err = patch::parse_pmm_scene_frame_range_patch_options(&[]).unwrap_err();
        assert!(
            err.contains("at least one patch option is required"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_pmm_scene_frame_range_patch_options_rejects_unknown_and_invalid_values() {
        let unknown = patch::parse_pmm_scene_frame_range_patch_options(&[
            "--unknown".to_string(),
            "1".to_string(),
        ])
        .unwrap_err();
        assert!(
            unknown.contains("unknown option"),
            "unexpected unknown-option error: {unknown}"
        );

        let missing_value =
            patch::parse_pmm_scene_frame_range_patch_options(&["--begin-frame".to_string()])
                .unwrap_err();
        assert!(
            missing_value.contains("missing value"),
            "unexpected missing-value error: {missing_value}"
        );

        let invalid_bool = patch::parse_pmm_scene_frame_range_patch_options(&[
            "--begin-frame-enabled".to_string(),
            "maybe".to_string(),
        ])
        .unwrap_err();
        assert!(
            invalid_bool.contains("invalid --begin-frame-enabled"),
            "unexpected invalid-bool error: {invalid_bool}"
        );
    }
}
