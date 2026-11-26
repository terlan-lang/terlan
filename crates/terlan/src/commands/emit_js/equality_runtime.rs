//! Structural equality support for JavaScript values emitted from Terlan.

pub(super) const FUNCTION: &str = "$terlan$equal";

const SOURCE: &str = r#"function $terlan$equal(left, right) {
  if (left === right) return true;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left) && Array.isArray(right) &&
      left.length === right.length &&
      left.every((value, index) => $terlan$equal(value, right[index]));
  }
  if (left === null || right === null ||
      typeof left !== "object" || typeof right !== "object" ||
      Object.getPrototypeOf(left) !== Object.prototype ||
      Object.getPrototypeOf(right) !== Object.prototype) {
    return false;
  }
  const leftKeys = Object.keys(left);
  const rightKeys = Object.keys(right);
  return leftKeys.length === rightKeys.length &&
    leftKeys.every(key => Object.prototype.hasOwnProperty.call(right, key) &&
      $terlan$equal(left[key], right[key]));
}

"#;

/// Adds the equality runtime only to modules that call it.
pub(super) fn attach(source: String) -> String {
    if source.contains("$terlan$equal(") {
        format!("{SOURCE}{source}")
    } else {
        source
    }
}

/// Returns whether an operator uses Terlan structural equality.
pub(super) fn is_equality(operator: &str) -> bool {
    matches!(operator, "==" | "=:=" | "!=" | "/=" | "=/=")
}

/// Returns whether a structural equality result must be negated.
pub(super) fn is_inequality(operator: &str) -> bool {
    matches!(operator, "!=" | "/=" | "=/=")
}

#[cfg(test)]
#[path = "equality_runtime_test.rs"]
mod tests;
