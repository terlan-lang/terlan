use super::{VmHttpSessionLiveTemplateSourceSpan, VmHttpSessionRuntime};
use crate::runtime::vm::ReplValue;

#[test]
fn http_session_rejects_unsupported_actor_patch_return_before_state_update() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let observed_version = sessions
        .state_version(&created.session)
        .expect("state version should read");
    let source = VmHttpSessionLiveTemplateSourceSpan::new(" app.Dashboard ", 12, 5)
        .expect("source span should validate");
    let mut update_ran = false;

    let error = sessions
        .fanout_live_template_state_update(
            &created.session,
            observed_version,
            "dashboard.patch",
            &source,
            ReplValue::List(vec![ReplValue::Iterator {
                items: vec![ReplValue::Int(1)],
                index: 0,
            }]),
            |runtime, session| {
                update_ran = true;
                runtime.write(session, "count", ReplValue::Int(1))
            },
        )
        .expect_err("stateful iterator payload must fail before actor update");

    assert_eq!(
        error,
        "invalid_template_actor_return_type: app.Dashboard:12:5: HTTP live-template actor return type Iterator cannot be serialized as a typed patch payload"
    );
    assert!(!update_ran);
    assert_eq!(
        sessions
            .state_version(&created.session)
            .expect("state version should remain readable"),
        observed_version
    );
    assert_eq!(
        sessions
            .read(&created.session, "count")
            .expect("actor table should remain readable"),
        None
    );
}
