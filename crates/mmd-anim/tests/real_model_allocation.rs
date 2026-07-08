use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    hint::black_box,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use mmd_anim::{
    format::{build_pair_clip, import_pmx_runtime, import_vmd_motion},
    runtime::RuntimeInstance,
};

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn reset_alloc_count() {
    ALLOC_COUNT.store(0, Ordering::SeqCst);
}

fn alloc_count() -> usize {
    ALLOC_COUNT.load(Ordering::SeqCst)
}

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

const LOCAL_ALLOC_PMX_ENV: &str = "MMD_ANIM_LOCAL_ALLOC_PMX";
const LOCAL_ALLOC_VMD_ENV: &str = "MMD_ANIM_LOCAL_ALLOC_VMD";
const LOCAL_ALLOC_FRAMES_ENV: &str = "MMD_ANIM_LOCAL_ALLOC_FRAMES";

fn local_alloc_pair_paths_from_env() -> Option<(PathBuf, PathBuf)> {
    let pmx = env::var(LOCAL_ALLOC_PMX_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)?;
    let vmd = env::var(LOCAL_ALLOC_VMD_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)?;
    Some((pmx, vmd))
}

fn local_alloc_frames_from_env() -> Vec<f32> {
    env::var(LOCAL_ALLOC_FRAMES_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .split(',')
                .filter_map(|part| part.trim().parse().ok())
                .collect()
        })
        .filter(|frames: &Vec<f32>| !frames.is_empty())
        .unwrap_or_else(|| vec![0.0, 15.0, 30.0])
}

fn assert_clip_evaluation_does_not_allocate_after_warmup(
    pmx_bytes: &[u8],
    vmd_bytes: &[u8],
    frames: &[f32],
) {
    let pmx = import_pmx_runtime(pmx_bytes).expect("pmx must import");
    let vmd = import_vmd_motion(vmd_bytes).expect("vmd must import");
    let clip = build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        pmx.model.ik_count(),
    );

    let model = Arc::new(pmx.model);
    let morph_count = model.morph_count() as usize;
    let ik_count = model.ik_count();
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, ik_count);

    runtime.evaluate_clip_frame(&clip, 0.0);

    reset_alloc_count();

    for &frame in frames {
        runtime.evaluate_clip_frame(&clip, frame);
        black_box(runtime.world_matrices());
        black_box(runtime.skinning_matrices());
        black_box(runtime.morph_weights());
    }

    assert_eq!(
        alloc_count(),
        0,
        "warmed real-model clip evaluation must not allocate"
    );
}

#[test]
fn real_fixture_clip_evaluation_does_not_allocate_after_warmup() {
    let pmx_bytes = std::fs::read(fixture_path(
        "../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx",
    ))
    .expect("pmx fixture must exist");
    let vmd_bytes = std::fs::read(fixture_path(
        "../mmd-anim-format/fixtures/vmd/ik_multi_bone_nondefault.vmd",
    ))
    .expect("vmd fixture must exist");

    assert_clip_evaluation_does_not_allocate_after_warmup(
        &pmx_bytes,
        &vmd_bytes,
        &[0.0, 15.0, 30.0, 45.0, 60.0],
    );
}

#[test]
#[ignore = "local large-asset allocation gate; requires MMD_ANIM_LOCAL_ALLOC_PMX and MMD_ANIM_LOCAL_ALLOC_VMD"]
fn local_large_asset_clip_evaluation_does_not_allocate_after_warmup() {
    let Some((pmx_path, vmd_path)) = local_alloc_pair_paths_from_env() else {
        eprintln!(
            "skip local_large_asset_clip_evaluation_does_not_allocate_after_warmup: set {LOCAL_ALLOC_PMX_ENV} and {LOCAL_ALLOC_VMD_ENV}"
        );
        return;
    };

    assert!(
        pmx_path.is_file(),
        "{LOCAL_ALLOC_PMX_ENV} must point to an existing PMX file: {}",
        pmx_path.display()
    );
    assert!(
        vmd_path.is_file(),
        "{LOCAL_ALLOC_VMD_ENV} must point to an existing VMD file: {}",
        vmd_path.display()
    );

    let pmx_bytes = std::fs::read(&pmx_path).expect("local pmx must be readable");
    let vmd_bytes = std::fs::read(&vmd_path).expect("local vmd must be readable");
    let frames = local_alloc_frames_from_env();

    assert_clip_evaluation_does_not_allocate_after_warmup(&pmx_bytes, &vmd_bytes, &frames);
}
