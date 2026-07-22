#include "include/native_boundary.hpp"

#include <atomic>
#include <stdexcept>

namespace terlan_fixture {
namespace {
std::atomic<std::int64_t> live_count{0};
std::atomic<std::int64_t> live_gauge_count{0};
}

NativeBoundary::NativeBoundary(std::int64_t value) noexcept : value_(value) {
  ++live_count;
}

NativeBoundary::~NativeBoundary() noexcept { --live_count; }

std::int64_t NativeBoundary::value() const noexcept { return value_; }

std::int64_t NativeBoundary::doubled() const noexcept { return value_ * 2; }

std::unique_ptr<std::string> NativeBoundary::label() const noexcept {
  return std::make_unique<std::string>(std::to_string(value_));
}

std::unique_ptr<std::vector<std::uint8_t>> NativeBoundary::bytes() const noexcept {
  return std::make_unique<std::vector<std::uint8_t>>(
      std::initializer_list<std::uint8_t>{static_cast<std::uint8_t>(value_),
                                          static_cast<std::uint8_t>(value_ + 1)});
}

std::unique_ptr<std::vector<std::int64_t>> NativeBoundary::samples() const noexcept {
  return std::make_unique<std::vector<std::int64_t>>(
      std::initializer_list<std::int64_t>{value_, value_ * 2});
}

BoundaryMode NativeBoundary::mode() const noexcept {
  if (value_ < 0) {
    return BoundaryMode::Raw;
  }
  if (value_ == 42) {
    return BoundaryMode::Doubled;
  }
  if (value_ == 0) {
    return BoundaryMode::Offset;
  }
  return BoundaryMode::Hidden;
}

std::int64_t NativeBoundary::tripled_or_throw() const {
  if (value_ < 0) {
    throw std::runtime_error("sensitive upstream exception payload");
  }
  return value_ * 3;
}

void NativeBoundary::add(std::int64_t delta) noexcept { value_ += delta; }

std::unique_ptr<NativeBoundary> make_native_boundary(std::int64_t value) noexcept {
  return std::make_unique<NativeBoundary>(value);
}

std::int64_t live_native_boundary_count() noexcept { return live_count.load(); }

std::int64_t sum_snapshot_fields(std::int64_t value,
                                 std::int64_t doubled) noexcept {
  return value + doubled;
}

std::int64_t sum_integer_list(
    rust::Slice<const std::int64_t> values) noexcept {
  std::int64_t total = 0;
  for (std::size_t index = 0; index < values.size(); ++index) {
    total += values[index];
  }
  return total;
}

double sum_float_list(rust::Slice<const double> values) noexcept {
  double total = 0.0;
  for (std::size_t index = 0; index < values.size(); ++index) {
    total += values[index];
  }
  return total;
}

std::unique_ptr<NativeSnapshot> make_native_snapshot(
    std::int64_t value) noexcept {
  auto snapshot = std::make_unique<NativeSnapshot>();
  snapshot->value = value;
  snapshot->doubled = value * 2;
  return snapshot;
}

NativeGauge::NativeGauge(std::int64_t value) noexcept : value_(value) {
  ++live_gauge_count;
}

NativeGauge::~NativeGauge() noexcept { --live_gauge_count; }

std::int64_t NativeGauge::reading() const noexcept { return value_; }

void NativeGauge::increment(std::int64_t delta) noexcept { value_ += delta; }

std::unique_ptr<NativeGauge> make_native_gauge(std::int64_t value) noexcept {
  return std::make_unique<NativeGauge>(value);
}

std::int64_t live_native_gauge_count() noexcept {
  return live_gauge_count.load();
}

}  // namespace terlan_fixture
