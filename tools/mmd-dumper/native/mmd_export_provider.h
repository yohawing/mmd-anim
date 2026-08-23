#pragma once

#include "mmd_oracle_types.h"

namespace mmd_oracle {

OracleRecord captureMmdExportSnapshot(double frame, const char* projectPath);
double getMmdExportFrame();

} // namespace mmd_oracle
