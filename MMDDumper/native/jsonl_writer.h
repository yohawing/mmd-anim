#pragma once

#include <ostream>
#include "mmd_oracle_types.h"

namespace mmd_oracle {

void writeOracleRecordJsonl(std::ostream& out, const OracleRecord& record);

} // namespace mmd_oracle
