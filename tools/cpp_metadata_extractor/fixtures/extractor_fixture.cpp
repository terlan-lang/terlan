#include "extractor_fixture.hpp"

namespace extractor_fixture {

std::int64_t Counter::value() const noexcept { return 0; }

void Counter::copy_to(std::int64_t* output) const noexcept {
  if (output != nullptr) {
    *output = value();
  }
}

Counter make_counter(std::int64_t) noexcept { return Counter{}; }

const Counter& borrow_counter() noexcept {
  static const Counter counter{};
  return counter;
}

std::int64_t choose(std::int64_t value) noexcept { return value; }

double choose(double value) noexcept { return value; }

std::int64_t may_throw() { return 0; }

void format_many(const char*, ...) {}

void visit(void (*)(std::int64_t)) noexcept {}

}  // namespace extractor_fixture
