use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand, ValueEnum};

mod commands;
mod mmd_dumper_oracle;
mod schema;
mod support;

pub(crate) use support::{
    copy_world_matrices_to_f32, diagnostics_suffix, f32_checksum, format_cli_error, io_error_label,
    read_file, read_text_file, resolve_maybe_absolute, translation_checksum,
    unsupported_format_error, write_file,
};

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
        long_about = "Run the runtime importer for a model, or a model/motion pair.\nUse this when checking runtime names, clip build stats, a single evaluated frame, or batch frame JSON for host comparisons.\n--frame-range uses inclusive START:END:STEP in MMD frame units.\n\nSupported formats: .pmx + .vmd, .pmd + .vmd",
        after_help = "Examples:\n  mmd-anim import model.pmx\n  mmd-anim import model.pmx motion.vmd --clip\n  mmd-anim import model.pmx motion.vmd --frame 120\n  mmd-anim import model.pmx motion.vmd --frames 0,30,60 --json\n  mmd-anim import model.pmx motion.vmd --frame-range 0:120:5 --json\n    (unit: MMD coordinate)"
    )]
    Import {
        /// Path to the PMX/PMD model file
        model: PathBuf,
        /// Optional path to the VMD motion file
        motion: Option<PathBuf>,
        /// Request JSON output where supported. Required with --frames or --frame-range.
        #[arg(long)]
        json: bool,
        /// Show clip build statistics for a model/motion pair
        #[arg(long)]
        clip: bool,
        /// Evaluate a single frame for a model/motion pair
        #[arg(long)]
        frame: Option<f32>,
        /// Evaluate multiple frames for a model/motion pair, as comma-separated MMD frame values
        #[arg(long, value_name = "LIST")]
        frames: Option<String>,
        /// Evaluate an inclusive frame range for a model/motion pair: START:END:STEP
        #[arg(long, value_name = "START:END:STEP")]
        frame_range: Option<String>,
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
        long_about = "Write an MMD asset to an output path, optionally starting from JSON.\nWith --from-json, the input must be UTF-8 JSON text and the output extension selects the binary format.\nThe JSON shape is the raw parsed DTO emitted by `mmd-anim inspect <asset> --json`, for example PmxParsedModel for .pmx, PmdParsedModel for .pmd, or VmdParsedAnimation for .vmd. For new VMD generation, use the raw VmdParsedAnimation DTO shape directly.\nUse this for parser/exporter smoke checks and JSON-to-binary conversion.\n\nSupported formats: .pmx, .pmd, .vmd",
        after_help = "Examples:\n  mmd-anim export input.vmd output.vmd\n  mmd-anim export input.json output.vmd --from-json\n  mmd-anim export motion-dto.json motion.vmd --from-json"
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

    /// Convert a PMX model to FBX 7.4 binary.
    #[command(
        name = "convert-fbx",
        long_about = "Convert a PMX model to a minimal FBX 7.4 binary file.\nWith --vmd, bone motion is baked to FBX AnimationStack/AnimationLayer/AnimCurve channels.\nUse --max-frame with --vmd to cap the inclusive bake range for local smoke checks.",
        after_help = "Examples:\n  mmd-anim convert-fbx model.pmx model.fbx\n  mmd-anim convert-fbx model.pmx model.fbx --vmd motion.vmd\n  mmd-anim convert-fbx model.pmx smoke.fbx --vmd motion.vmd --max-frame 120"
    )]
    ConvertFbx {
        /// Path to the input PMX model file
        input: PathBuf,
        /// Path to the output FBX file
        output: PathBuf,
        /// Optional VMD motion file to bake as FBX animation
        #[arg(long)]
        vmd: Option<PathBuf>,
        /// Optional inclusive maximum frame for FBX runtime bake smoke checks
        #[arg(long)]
        max_frame: Option<u32>,
    },

    /// Build a PMX model from a parts manifest.
    #[command(
        name = "build-pmx",
        long_about = "Create a PMX model from a parts manifest JSON.\nThe input is not the parsed PmxParsedModel DTO used by export --from-json; it is a PmxPartsDescriptor plus flat positionsXyz, normalsXyz, uvsXy, indices, and optional skin/edge arrays.\n\nSupported formats: .json → .pmx",
        after_help = "Examples:\n  mmd-anim build-pmx parts.json model.pmx\n  mmd-anim build-pmx ./fixtures/parts.json ./out/model.pmx"
    )]
    BuildPmx {
        /// Path to the PMX parts manifest JSON file
        input: PathBuf,
        /// Path to the output PMX file
        output: PathBuf,
    },

    /// Sample a VMD camera, light, or self-shadow track at one frame.
    #[command(
        name = "vmd-sample",
        long_about = "Sample a VMD camera, light, or self-shadow track at a frame.\nUse this to get canonical parser/runtime-independent values for preview, fixture, or host comparison workflows.\n\nSupported formats: .vmd",
        after_help = "Examples:\n  mmd-anim vmd-sample motion.vmd --kind camera --frame 120\n  mmd-anim vmd-sample motion.vmd --kind light --frame 20 --json\n  mmd-anim vmd-sample motion.vmd --kind self-shadow --frame 20"
    )]
    VmdSample {
        /// Path to the VMD motion file
        motion: PathBuf,
        /// Track kind to sample
        #[arg(long, value_enum)]
        kind: commands::vmd_sample::VmdSampleKind,
        /// Frame to sample, in MMD frame units
        #[arg(long)]
        frame: f32,
        /// Output sampled state as JSON
        #[arg(long)]
        json: bool,
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
            frames,
            frame_range,
        }) => dispatch_import(
            &model,
            motion.as_deref(),
            json,
            clip,
            frame,
            frames,
            frame_range,
        ),
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
        Some(Commands::ConvertFbx {
            input,
            output,
            vmd,
            max_frame,
        }) => commands::fbx::convert_pmx_to_fbx(&input, &output, vmd.as_deref(), max_frame),
        Some(Commands::BuildPmx { input, output }) => {
            commands::export::export_pmx_from_parts_manifest(&input, &output)
        }
        Some(Commands::VmdSample {
            motion,
            kind,
            frame,
            json,
        }) => dispatch_vmd_sample(&motion, kind, frame, json),
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

fn dispatch_vmd_sample(
    motion: &Path,
    kind: commands::vmd_sample::VmdSampleKind,
    frame: f32,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if !frame.is_finite() {
        return usage_error("vmd-sample --frame must be finite");
    }
    commands::vmd_sample::vmd_sample(motion, kind, frame, json)
}

fn dispatch_import(
    model: &Path,
    motion: Option<&Path>,
    use_json: bool,
    show_clip: bool,
    frame: Option<f32>,
    frames: Option<String>,
    frame_range: Option<String>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let batch_requested = frames.is_some() || frame_range.is_some();
    if use_json && !batch_requested {
        return usage_error("import --json is only supported with --frames or --frame-range");
    }
    if show_clip && frame.is_some() {
        return usage_error("import --clip and --frame cannot be combined");
    }
    if show_clip && batch_requested {
        return usage_error("import --clip cannot be combined with --frames or --frame-range");
    }
    if frame.is_some() && batch_requested {
        return usage_error("import --frame cannot be combined with --frames or --frame-range");
    }
    if frames.is_some() && frame_range.is_some() {
        return usage_error("import --frames and --frame-range cannot be combined");
    }
    if batch_requested && !use_json {
        return usage_error("import --frames and --frame-range require --json");
    }

    if let Some(motion) = motion {
        if let Some(frames) = frames {
            let frame_spec = match commands::import::parse_import_frames_list(&frames) {
                Ok(frame_spec) => frame_spec,
                Err(error) => return usage_error(error),
            };
            return commands::import::import_pair_frames_json(model, motion, frame_spec);
        }
        if let Some(frame_range) = frame_range {
            let frame_spec = match commands::import::parse_import_frame_range(&frame_range) {
                Ok(frame_spec) => frame_spec,
                Err(error) => return usage_error(error),
            };
            return commands::import::import_pair_frames_json(model, motion, frame_spec);
        }
        if show_clip {
            return commands::import::import_pair_clip_summary(model, motion);
        }
        if let Some(frame) = frame {
            return commands::import::import_pair_frame_summary(model, motion, frame);
        }
        return commands::import::import_pair_summary(model, motion);
    }

    if show_clip || frame.is_some() || batch_requested {
        return usage_error(
            "import --clip, --frame, --frames, and --frame-range require a motion argument",
        );
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

    let diagnose_options = match commands::compare::parse_diagnose_numeric_bone_rest(rest, frame) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{}", commands::compare::DIAGNOSE_NUMERIC_BONE_USAGE);
            return Ok(ExitCode::from(2));
        }
    };
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

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;
