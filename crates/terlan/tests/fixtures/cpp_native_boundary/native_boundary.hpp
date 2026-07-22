#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "rust/cxx.h"

#define TERLAN_FIXTURE_UNSAFE_SCALE(value) ((value) * 2)

namespace terlan_fixture {

struct NativeSnapshot final {
  std::int64_t value;
  std::int64_t doubled;

  std::int64_t projected_value() const noexcept { return value; }
  std::int64_t projected_doubled() const noexcept { return doubled; }
};

enum class BoundaryMode : std::int32_t {
  Raw = 7,
  Doubled = 41,
  Offset = 99,
  Hidden = 123,
};

class NativeBoundary final {
 public:
  explicit NativeBoundary(std::int64_t value) noexcept;
  ~NativeBoundary() noexcept;

  NativeBoundary(const NativeBoundary&) = delete;
  NativeBoundary& operator=(const NativeBoundary&) = delete;

  std::int64_t value() const noexcept;
  std::int64_t doubled() const noexcept;
  std::unique_ptr<std::string> label() const noexcept;
  std::unique_ptr<std::vector<std::uint8_t>> bytes() const noexcept;
  std::unique_ptr<std::vector<std::int64_t>> samples() const noexcept;
  BoundaryMode mode() const noexcept;
  std::int64_t tripled_or_throw() const;
  void add(std::int64_t delta) noexcept;

 private:
  std::int64_t value_;
};

std::unique_ptr<NativeBoundary> make_native_boundary(std::int64_t value) noexcept;
std::int64_t live_native_boundary_count() noexcept;
std::int64_t sum_snapshot_fields(std::int64_t value,
                                 std::int64_t doubled) noexcept;
std::int64_t sum_integer_list(
    rust::Slice<const std::int64_t> values) noexcept;
double sum_float_list(rust::Slice<const double> values) noexcept;
std::unique_ptr<NativeSnapshot> make_native_snapshot(
    std::int64_t value) noexcept;

class NativeGauge final {
 public:
  explicit NativeGauge(std::int64_t value) noexcept;
  ~NativeGauge() noexcept;

  NativeGauge(const NativeGauge&) = delete;
  NativeGauge& operator=(const NativeGauge&) = delete;

  std::int64_t reading() const noexcept;
  void increment(std::int64_t delta) noexcept;

 private:
  std::int64_t value_;
};

std::unique_ptr<NativeGauge> make_native_gauge(std::int64_t value) noexcept;
std::int64_t live_native_gauge_count() noexcept;

}  // namespace terlan_fixture
