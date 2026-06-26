//! Internal schema types used by CLI diagnostics.

#[allow(unused_imports)]
pub(crate) use crate::mmd_dumper_oracle::{
    DEFAULT_FOCUSED_IK_BONE_NAMES, GoldenIkBatchCase, GoldenIkBatchManifest, GoldenIkFixture,
    MmdDumperOracleBone, MmdDumperOracleDump, MmdDumperOracleFrame, MmdDumperOracleModel,
    MmdDumperOracleMorph, MmdDumperOracleParseError, MmdDumperOracleSource,
};

pub(crate) const SCHEMA_VERSION: u32 = 1;
