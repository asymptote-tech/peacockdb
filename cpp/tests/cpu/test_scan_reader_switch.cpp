// PEACOCK_SCAN_READER, the measurement switch between the scan's two readers: unset or
// "chunked" is the chunked reader, "read_parquet" the whole-file call, anything else refused by
// name. It is read once per process. No device needed.

#include "peacock/operators.h"

#include <gtest/gtest.h>

#include <cstdlib>
#include <stdexcept>
#include <string>

using peacock::ScanReader;

TEST(ScanReader, TheSwitchIsReadOnce) {
  if (std::getenv("PEACOCK_SCAN_READER")) GTEST_SKIP() << "PEACOCK_SCAN_READER is set";
  EXPECT_EQ(peacock::scan_reader(), ScanReader::Chunked);
  ASSERT_EQ(setenv("PEACOCK_SCAN_READER", "read_parquet", 1), 0);
  EXPECT_EQ(peacock::scan_reader(), ScanReader::Chunked);
  unsetenv("PEACOCK_SCAN_READER");
}

TEST(ScanReader, TheSwitchNamesTwoReaders) {
  EXPECT_EQ(peacock::parse_scan_reader(nullptr), ScanReader::Chunked);
  EXPECT_EQ(peacock::parse_scan_reader("chunked"), ScanReader::Chunked);
  EXPECT_EQ(peacock::parse_scan_reader("read_parquet"), ScanReader::ReadParquet);
}

TEST(ScanReader, AnyOtherValueIsRefusedByName) {
  for (const char* value : {"", "Chunked", "read-parquet", "chunked "}) {
    try {
      peacock::parse_scan_reader(value);
      ADD_FAILURE() << "\"" << value << "\" was accepted";
    } catch (const std::runtime_error& e) {
      EXPECT_NE(std::string(e.what()).find("PEACOCK_SCAN_READER=\"" + std::string(value) + "\""),
                std::string::npos)
          << e.what();
    }
  }
}
