#pragma once

#include <cstdint>

#if defined(__clang__)
#define TERLAN_BIND __attribute__((annotate("TERLAN_BIND")))
#define TERLAN_OUT __attribute__((annotate("CV_OUT")))
#else
#define TERLAN_BIND
#define TERLAN_OUT
#endif

namespace extractor_fixture {

/// A tiny value used to validate normalized C++ declaration metadata.
class TERLAN_BIND Counter final {
 public:
  /// Returns the current counter value.
  std::int64_t value() const noexcept;

  /// Writes the current value through an intentionally unsupported pointer.
  void copy_to(TERLAN_OUT std::int64_t* output) const noexcept;

 private:
  std::int64_t value_;
};

/// Creates one counter value.
TERLAN_BIND Counter make_counter(std::int64_t initial) noexcept;

/// Returns an intentionally borrowed reference.
const Counter& borrow_counter() noexcept;

/// Represents an unresolved overload set.
std::int64_t choose(std::int64_t value) noexcept;

/// Represents an unresolved overload set.
double choose(double value) noexcept;

/// Represents a function that can throw.
std::int64_t may_throw();

/// Represents an unsupported variadic function.
void format_many(const char* format, ...);

/// Represents an unsupported callback parameter.
void visit(void (*callback)(std::int64_t)) noexcept;

/// Represents an unspecialized template declaration.
template <typename T>
T identity(T value) noexcept;

/// Defines non-sequential values for symbolic enum extraction.
enum class CounterMode : std::int32_t {
  Raw = 7,
  Doubled = 41,
  Offset = 99,
};

}  // namespace extractor_fixture
