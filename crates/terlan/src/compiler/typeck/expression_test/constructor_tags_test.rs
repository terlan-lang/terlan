//! Constructor coverage must use the same atom spelling as the language.

use super::*;

fn constructor_case(name: &str, tag: &str, branches: &str) -> Vec<Diagnostic> {
    check_syntax_output(&format!(
        "module fixture.ConstructorCoverage.\n\
         pub type Message = {{Atom[\"idle\"], Int}} | {{Atom[\"{tag}\"], Bool}}.\n\
         pub constructor Idle {{ (sequence: Int): Message -> {{Atom[\"idle\"], sequence}} }}.\n\
         pub constructor {name} {{ (flag: Bool): Message -> {{Atom[\"{tag}\"], flag}} }}.\n\
         pub inspect(message: Message): Bool -> case message {{ {branches} }}.\n"
    ))
}

#[test]
fn multiword_constructor_union_cases_use_canonical_tags() {
    for (name, tag) in [
        ("SendPacket", "send_packet"),
        ("HTTPError", "http_error"),
        ("Frame2D", "frame_2_d"),
    ] {
        for branches in [
            format!("Idle(_) -> false; {name}(flag) -> flag"),
            format!("{name}(false) -> false; Idle(_) -> false; {name}(true) -> true"),
        ] {
            let diagnostics = constructor_case(name, tag, &branches);
            assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
        }
    }
}

#[test]
fn multiword_constructor_coverage_preserves_missing_and_guarded_payloads() {
    for branches in [
        "Idle(_) -> false",
        "Idle(_) -> false; SendPacket(true) -> true",
        "Idle(_) -> false; SendPacket(true) -> true; SendPacket(true) -> false",
        "Idle(_) -> false; SendPacket(true) -> true; SendPacket(false) where false -> false",
    ] {
        let diagnostics = constructor_case("SendPacket", "send_packet", branches);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("non-exhaustive case")),
            "incomplete case accepted: {branches}: {diagnostics:?}"
        );
    }
}

#[test]
fn multiword_atom_constructors_cover_closed_union() {
    let diagnostics = check_syntax_output(
        "module fixture.AtomCoverage.\n\
         pub type Signal = Atom[\"ready_now\"] | Atom[\"http_error\"].\n\
         pub constructor ReadyNow { (): Signal -> Atom[\"ready_now\"] }.\n\
         pub constructor HTTPError { (): Signal -> Atom[\"http_error\"] }.\n\
         pub inspect(signal: Signal): Bool -> case signal { ReadyNow -> true; HTTPError -> false }.\n"
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
