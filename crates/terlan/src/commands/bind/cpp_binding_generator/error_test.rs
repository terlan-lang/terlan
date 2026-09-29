use super::*;

#[test]
fn binding_errors_preserve_diagnostic_identity_and_rendering() {
    let message = "error[cpp.binding.policy]: missing ownership policy";
    let error = CppBindingError::from(message);
    assert_eq!(error.0.domain(), ErrorDomain::CommandExecution);
    assert_eq!(error.0.code(), "cpp.binding.policy");
    assert_eq!(error.0.operation(), "generate C++ bindings");
    assert_eq!(String::from(error), message);
}
