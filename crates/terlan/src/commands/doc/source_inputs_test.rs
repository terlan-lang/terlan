use super::*;
use crate::commands::doc::doc_test::make_doc_command_test_dir;

#[test]
fn preparation_preserves_input_order_across_workers() {
    let directory = make_doc_command_test_dir("parallel_doc_sources");
    let files = (0..6)
        .map(|index| {
            let path = directory.join(format!("Module{index}.terl"));
            std::fs::write(
                &path,
                format!("module parallel.module{index}.\n\npub value(): Int ->\n    {index}.\n"),
            )
            .unwrap();
            path
        })
        .collect::<Vec<_>>();

    let prepared = prepare(files)
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(prepared.len(), 6);
    for (index, prepared) in prepared.iter().enumerate() {
        assert_eq!(
            prepared.syntax_output.module_name,
            format!("parallel.module{index}")
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn preparation_retains_the_failing_path_and_span() {
    let directory = make_doc_command_test_dir("parallel_doc_failure");
    let path = directory.join("Broken.terl");
    std::fs::write(&path, "module broken\n").unwrap();

    let failure = prepare(vec![path.clone()]).pop().unwrap().unwrap_err();
    match failure {
        Failure::Parse {
            path: failed,
            start,
            end,
            ..
        } => {
            assert_eq!(failed, path.to_string_lossy());
            assert!(end >= start);
        }
        _ => panic!("expected parse failure"),
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn preparation_reports_unreadable_input_without_losing_other_results() {
    let directory = make_doc_command_test_dir("parallel_doc_read_failure");
    let missing = directory.join("Missing.terl");
    let prepared = prepare(vec![missing]);
    assert_eq!(prepared.len(), 1);
    assert!(matches!(&prepared[0], Err(Failure::Message(message)) if !message.is_empty()));
    std::fs::remove_dir_all(directory).unwrap();
}
