from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))

from check_python_abi_drift import HEADER, check_header  # noqa: E402


class AbiDriftCheckerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.header = HEADER.read_text(encoding="utf-8")

    def test_current_header_matches_manifest(self) -> None:
        errors, unwrapped = check_header(self.header)
        self.assertEqual(errors, [])
        self.assertGreater(len(unwrapped), 0)

    def test_function_signature_mutation_is_detected(self) -> None:
        mutated = self.header.replace(
            "size_t         len);",
            "uint32_t       len);",
            1,
        )
        errors, _ = check_header(mutated)
        self.assertTrue(
            any("mmd_runtime_parse_vmd_json" in error for error in errors), errors
        )

    def test_struct_shape_mutation_is_detected(self) -> None:
        mutated = self.header.replace(
            "typedef struct mmd_runtime_ffi_rig_ik_link {\n"
            "    uint32_t bone_slot;\n"
            "    uint8_t  has_angle_limit; /* must be 0 or 1 */\n"
            "    float    angle_limit_min_xyz[3];\n"
            "    float    angle_limit_max_xyz[3];",
            "typedef struct mmd_runtime_ffi_rig_ik_link {\n"
            "    uint32_t bone_slot;\n"
            "    uint8_t  has_angle_limit; /* must be 0 or 1 */\n"
            "    float    angle_limit_min_xyz[3];\n"
            "    float    angle_limit_max_xyz[4];",
            1,
        )
        errors, _ = check_header(mutated)
        self.assertTrue(
            any("mmd_runtime_ffi_rig_ik_link_t" in error for error in errors), errors
        )


if __name__ == "__main__":
    unittest.main()
