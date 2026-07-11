//! Renderer-independent MMD animation runtime core.
//!
//! The core crate intentionally has no Wasm, Unity, parser, or renderer
//! dependencies. Wrappers pass pre-normalized model and animation IR into this
//! crate and read contiguous output buffers back.

mod animation;
pub mod append_primitive;
mod flat_model;
pub mod ik_primitive;
mod model;
mod pose;
mod runtime;

pub use animation::{
    AnimationClip, AnimationClipBuilder, BoneAnimationBinding, BoneSample, ClipFrameBounds,
    ClipSample, InterpolationScalar, InterpolationVector3, MorphAnimationBinding, MorphKeyframe,
    MorphSample, MorphTrack, MovableBoneKeyframe, MovableBoneTrack, PropertyAnimationBinding,
    PropertyKeyframe,
};
pub use append_primitive::{AppendPrimitiveInput, AppendPrimitiveOutput, solve_append_transform};
pub use flat_model::{
    FlatAppendTransformInput, FlatBoneInput, FlatBoneMorphInput, FlatGroupMorphInput,
    FlatIkLinkInput, FlatIkSolverInput, FlatModelInputError, FlatMorphInput,
    build_append_transforms_from_flat, build_append_transforms_from_flat_iter,
    build_bones_from_flat, build_ik_solvers_from_flat, build_ik_solvers_from_flat_iter,
    build_morph_init_from_flat, build_morph_init_from_flat_iter,
};
pub use ik_primitive::{
    IkChainDefinition, IkChainLinkDefinition, IkChainPoseInput, IkChainSolveOutput, IkChainSolver,
};
pub use model::{
    AppendTransform, AppendTransformInit, BoneIndex, BoneInit, BoneMorphOffset, GroupMorphOffset,
    IkAngleLimit, IkLink, IkLinkInit, IkSolver, IkSolverInit, LocalAxis, ModelArena,
    ModelBuildError, MorphIndex, MorphInit, MorphOffsetSpan, VertexMorphOffset,
};
pub use pose::PoseArena;
pub use runtime::{
    IkSolveOptions, IkSolverRuntimeStats, PhysicsMode, PhysicsStepStats, PhysicsTickConfig,
    RuntimeInstance,
};
