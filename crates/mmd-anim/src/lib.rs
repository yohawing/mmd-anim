//! Umbrella crate for `mmd-anim`.
//!
//! This crate is the main public dependency for applications that want MMD
//! format import/export and runtime evaluation from one package. Lower-level
//! crates remain available for users who want narrower dependencies.

pub use mmd_anim_format as format;
pub use mmd_anim_runtime as runtime;
