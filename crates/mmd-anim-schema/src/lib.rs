//! Shared schema crate.
//!
//! This crate will hold stable model IR, animation IR, and fixture trace types.
//! Keep it free of runtime evaluator state.

mod mmd_dumper_oracle;

pub use mmd_dumper_oracle::{
    DEFAULT_FOCUSED_IK_BONE_NAMES, GoldenIkBatchCase, GoldenIkBatchManifest, GoldenIkFixture,
    MmdDumperOracleBone, MmdDumperOracleDump, MmdDumperOracleFrame, MmdDumperOracleModel,
    MmdDumperOracleMorph, MmdDumperOracleParseError, MmdDumperOracleSource,
};

pub const SCHEMA_VERSION: u32 = 1;
