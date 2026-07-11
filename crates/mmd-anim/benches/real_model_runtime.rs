use std::{
    env,
    error::Error,
    fs::{self, File},
    hint::black_box,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use mmd_anim::{
    format::{build_pair_clip, import_pmx_runtime, import_vmd_motion},
    runtime::RuntimeInstance,
};
use serde_json::json;

const PMX_ENV: &str = "MMD_ANIM_REAL_BENCH_PMX";
const VMD_ENV: &str = "MMD_ANIM_REAL_BENCH_VMD";
const OUTPUT_ENV: &str = "MMD_ANIM_REAL_BENCH_OUTPUT";
const DEFAULT_START_FRAME: u32 = 0;
const DEFAULT_FRAME_COUNT: u32 = 120;
const DEFAULT_STEP: u32 = 1;
const MODEL_COUNTS: [usize; 3] = [1, 10, 30];

struct BenchConfig {
    pmx_path: PathBuf,
    vmd_path: PathBuf,
    output_path: Option<PathBuf>,
    start_frame: u32,
    frame_count: u32,
    step: u32,
}

struct SetupTimings {
    import_pmx: Duration,
    import_vmd: Duration,
    build_clip: Duration,
}

struct ScenarioTimings {
    init: Duration,
    warmup: Duration,
    eval: Duration,
    world_copy: Duration,
    skinning_copy: Duration,
    morph_copy: Duration,
}

fn main() -> Result<(), Box<dyn Error>> {
    let Some(config) = BenchConfig::from_env()? else {
        return Ok(());
    };

    let pmx_bytes = fs::read(&config.pmx_path)?;
    let vmd_bytes = fs::read(&config.vmd_path)?;

    let import_pmx_started = Instant::now();
    let pmx = import_pmx_runtime(&pmx_bytes)?;
    let import_pmx = import_pmx_started.elapsed();

    let import_vmd_started = Instant::now();
    let vmd = import_vmd_motion(&vmd_bytes)?;
    let import_vmd = import_vmd_started.elapsed();

    let build_clip_started = Instant::now();
    let clip = build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        pmx.model.ik_count(),
    );
    let build_clip = build_clip_started.elapsed();

    let setup = SetupTimings {
        import_pmx,
        import_vmd,
        build_clip,
    };
    let model = Arc::new(pmx.model);
    let bone_count = model.bone_count();
    let morph_count = model.morph_count() as usize;
    let ik_count = model.ik_count();
    let frames = config.frames();
    let pmx_path = config.pmx_path.display().to_string();
    let vmd_path = config.vmd_path.display().to_string();

    let mut output = config.open_output()?;
    for model_count in MODEL_COUNTS {
        let timings = run_scenario(
            model.clone(),
            &clip,
            model_count,
            morph_count,
            ik_count,
            &frames,
        );
        let record = json!({
            "kind": "realModelRuntimeBench",
            "status": "ok",
            "scenario": {
                "modelCount": model_count,
            },
            "frameRange": {
                "startFrame": config.start_frame,
                "frameCount": config.frame_count,
                "step": config.step,
            },
            "iterations": frames.len() * model_count,
            "model": {
                "boneCount": bone_count,
                "morphCount": morph_count,
                "ikCount": ik_count,
            },
            "timingsMsPerFrame": {
                "evalMs": ms_per_frame(timings.eval, frames.len()),
                "worldCopyMs": ms_per_frame(timings.world_copy, frames.len()),
                "skinningCopyMs": ms_per_frame(timings.skinning_copy, frames.len()),
                "morphCopyMs": ms_per_frame(timings.morph_copy, frames.len()),
            },
            "setupMs": {
                "importPmxMs": ms(setup.import_pmx),
                "importVmdMs": ms(setup.import_vmd),
                "buildClipMs": ms(setup.build_clip),
                "runtimeInitMs": ms(timings.init),
                "warmupMs": ms(timings.warmup),
            },
            "assets": {
                "pmx": &pmx_path,
                "vmd": &vmd_path,
            },
        });
        emit_jsonl(&record, output.as_mut())?;
    }

    Ok(())
}

impl BenchConfig {
    fn from_env() -> Result<Option<Self>, Box<dyn Error>> {
        let pmx_path = env::var_os(PMX_ENV).map(PathBuf::from);
        let vmd_path = env::var_os(VMD_ENV).map(PathBuf::from);
        let mut missing_env = Vec::new();
        if pmx_path.is_none() {
            missing_env.push(PMX_ENV);
        }
        if vmd_path.is_none() {
            missing_env.push(VMD_ENV);
        }
        if !missing_env.is_empty() {
            emit_skip(&["missing required env"], &missing_env)?;
            return Ok(None);
        }
        let pmx_path = pmx_path.expect("checked required PMX env");
        let vmd_path = vmd_path.expect("checked required VMD env");
        let output_path = env::var_os(OUTPUT_ENV).map(PathBuf::from);
        let start_frame = parse_u32_env(
            "MMD_ANIM_REAL_BENCH_START_FRAME",
            DEFAULT_START_FRAME,
            false,
        )?;
        let frame_count =
            parse_u32_env("MMD_ANIM_REAL_BENCH_FRAME_COUNT", DEFAULT_FRAME_COUNT, true)?;
        let step = parse_u32_env("MMD_ANIM_REAL_BENCH_STEP", DEFAULT_STEP, true)?;

        let mut missing_assets = Vec::new();
        if !pmx_path.is_file() {
            missing_assets.push(PMX_ENV);
        }
        if !vmd_path.is_file() {
            missing_assets.push(VMD_ENV);
        }
        if !missing_assets.is_empty() {
            emit_skip(&["asset path does not exist"], &missing_assets)?;
            return Ok(None);
        }

        Ok(Some(Self {
            pmx_path,
            vmd_path,
            output_path,
            start_frame,
            frame_count,
            step,
        }))
    }

    fn frames(&self) -> Vec<f32> {
        (0..self.frame_count)
            .map(|i| self.start_frame + i * self.step)
            .map(|frame| frame as f32)
            .collect()
    }

    fn open_output(&self) -> Result<Option<BufWriter<File>>, Box<dyn Error>> {
        self.output_path
            .as_ref()
            .map(|path| create_output_file(path).map(BufWriter::new))
            .transpose()
            .map_err(Into::into)
    }
}

fn create_output_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    File::create(path)
}

fn run_scenario(
    model: Arc<mmd_anim::runtime::ModelArena>,
    clip: &mmd_anim::runtime::AnimationClip,
    model_count: usize,
    morph_count: usize,
    ik_count: usize,
    frames: &[f32],
) -> ScenarioTimings {
    let init_started = Instant::now();
    let mut runtimes = (0..model_count)
        .map(|_| RuntimeInstance::new_with_counts(model.clone(), morph_count, ik_count))
        .collect::<Vec<_>>();
    let init = init_started.elapsed();

    let warmup_started = Instant::now();
    if let Some(&first_frame) = frames.first() {
        for runtime in &mut runtimes {
            runtime.evaluate_clip_frame(clip, first_frame);
        }
    }
    let warmup = warmup_started.elapsed();

    let mut eval = Duration::ZERO;
    let mut world_copy = Duration::ZERO;
    let mut skinning_copy = Duration::ZERO;
    let mut morph_copy = Duration::ZERO;
    let mut world_buffer = vec![0.0f32; model.bone_count() * 16 * model_count];
    let mut skinning_buffer = vec![0.0f32; model.bone_count() * 16 * model_count];
    let mut morph_buffer = vec![0.0f32; morph_count * model_count];

    for &frame in frames {
        let eval_started = Instant::now();
        for runtime in &mut runtimes {
            runtime.evaluate_clip_frame(clip, frame);
        }
        eval += eval_started.elapsed();

        let world_started = Instant::now();
        copy_world_matrices_from_runtimes(&runtimes, &mut world_buffer);
        black_box(&world_buffer);
        world_copy += world_started.elapsed();

        let skinning_started = Instant::now();
        copy_skinning_matrices_from_runtimes(&runtimes, &mut skinning_buffer);
        black_box(&skinning_buffer);
        skinning_copy += skinning_started.elapsed();

        let morph_started = Instant::now();
        copy_morphs_from_runtimes(&runtimes, &mut morph_buffer);
        black_box(&morph_buffer);
        morph_copy += morph_started.elapsed();
    }

    ScenarioTimings {
        init,
        warmup,
        eval,
        world_copy,
        skinning_copy,
        morph_copy,
    }
}

fn copy_world_matrices_from_runtimes(runtimes: &[RuntimeInstance], buffer: &mut [f32]) {
    let mut offset = 0;
    for runtime in runtimes {
        for matrix in runtime.world_matrices() {
            buffer[offset..offset + 16].copy_from_slice(&matrix.to_cols_array());
            offset += 16;
        }
    }
}

fn copy_skinning_matrices_from_runtimes(runtimes: &[RuntimeInstance], buffer: &mut [f32]) {
    let mut offset = 0;
    for runtime in runtimes {
        for matrix in runtime.skinning_matrices() {
            buffer[offset..offset + 16].copy_from_slice(&matrix.to_cols_array());
            offset += 16;
        }
    }
}

fn copy_morphs_from_runtimes(runtimes: &[RuntimeInstance], buffer: &mut [f32]) {
    let mut offset = 0;
    for runtime in runtimes {
        let weights = runtime.morph_weights();
        buffer[offset..offset + weights.len()].copy_from_slice(weights);
        offset += weights.len();
    }
}

fn parse_u32_env(name: &str, default: u32, non_zero: bool) -> Result<u32, Box<dyn Error>> {
    let Some(value) = env::var_os(name) else {
        return Ok(default);
    };
    let parsed = value
        .to_string_lossy()
        .parse::<u32>()
        .map_err(|err| format!("{name} must be a u32: {err}"))?;
    if non_zero && parsed == 0 {
        return Err(format!("{name} must be greater than zero").into());
    }
    Ok(parsed)
}

fn emit_skip(reasons: &[&str], missing: &[&str]) -> Result<(), Box<dyn Error>> {
    let record = json!({
        "kind": "realModelRuntimeBench",
        "status": "skipped",
        "reasons": reasons,
        "missing": missing,
    });
    let mut output = env::var_os(OUTPUT_ENV)
        .map(|path| create_output_file(path.as_ref()))
        .transpose()?
        .map(BufWriter::new);
    emit_jsonl(&record, output.as_mut())
}

fn emit_jsonl(
    record: &serde_json::Value,
    output: Option<&mut BufWriter<File>>,
) -> Result<(), Box<dyn Error>> {
    let line = serde_json::to_string(record)?;
    println!("{line}");
    if let Some(output) = output {
        writeln!(output, "{line}")?;
    }
    Ok(())
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn ms_per_frame(duration: Duration, frame_count: usize) -> f64 {
    ms(duration) / frame_count.max(1) as f64
}
