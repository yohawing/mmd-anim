//! Renderer-independent MMD animation runtime core.
//!
//! The core crate intentionally has no Wasm, Unity, parser, or renderer
//! dependencies. Wrappers pass pre-normalized model and animation IR into this
//! crate and read contiguous output buffers back.

mod animation;
pub mod append_primitive;
pub mod ik_primitive;
mod model;
mod pose;
mod runtime;

pub use animation::{
    AnimationClip, BoneAnimationBinding, InterpolationScalar, InterpolationVector3,
    MorphAnimationBinding, MorphKeyframe, MorphTrack, MovableBoneKeyframe, MovableBoneTrack,
    PropertyAnimationBinding, PropertyKeyframe,
};
pub use append_primitive::{AppendPrimitiveInput, AppendPrimitiveOutput, solve_append_transform};
pub use ik_primitive::{
    IkChainDefinition, IkChainLinkDefinition, IkChainPoseInput, IkChainSolveOutput, IkChainSolver,
};
pub use model::{
    AppendTransform, AppendTransformInit, BoneIndex, BoneInit, BoneMorphOffset, GroupMorphOffset,
    IkAngleLimit, IkLink, IkLinkInit, IkSolver, IkSolverInit, ModelArena, ModelBuildError,
    MorphIndex, MorphInit, MorphOffsetSpan, VertexMorphOffset,
};
pub use pose::PoseArena;
pub use runtime::{IkSolveOptions, IkSolverRuntimeStats, RuntimeInstance};
