use std::{
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
        after_help = "Examples:\n  mmd-anim verify oracle.jsonl\n  mmd-anim verify manifest.json --mode numeric\n  mmd-anim verify manifest.json --mode numeric --json\n  mmd-anim verify camera-manifest.json --mode camera\n  mmd-anim verify golden-root --mode ik\n  mmd-anim verify golden-root --mode ik --compare\n  mmd-anim verify golden-root --mode parser\n  mmd-anim verify manifest.json --mode numeric --diagnose case-a 120 左足ＩＫ"
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
        VerifyMode::Numeric => {
            if compare {
                return usage_error("verify --mode numeric does not support --compare");
            }
            if sample_frame_offset.is_some() {
                return usage_error("verify --mode numeric does not support --sample-frame-offset");
            }
            if use_json {
                if diagnose.is_some() || eval_frame.is_some() {
                    return usage_error(
                        "verify --mode numeric --json cannot be combined with --diagnose or --eval-frame",
                    );
                }
                return commands::compare::compare_numeric_manifest_json(target);
            }
            if let Some(parts) = diagnose {
                return dispatch_numeric_diagnose(target, parts, eval_frame);
            }
            if eval_frame.is_some() {
                return usage_error("verify --eval-frame requires --diagnose");
            }
            commands::compare::compare_numeric_manifest(target)
        }
        VerifyMode::Camera => {
            if compare || use_json {
                return usage_error("verify --mode camera does not support --compare or --json");
            }
            if sample_frame_offset.is_some() {
                return usage_error("verify --mode camera does not support --sample-frame-offset");
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
            commands::golden::golden_parser_summary(target)
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

    commands::golden::golden_ik_summary(root)
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

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;
