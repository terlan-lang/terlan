//! Portable runner fixture for the `.terls` command/exit-status handoff test.

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 5);
    assert_eq!(args[0], "run");
    assert!(std::path::Path::new(&args[1]).is_file());
    assert_eq!(args[2], "--script-eval");
    assert_eq!(args[3], "--");
    let code: u8 = args[4].to_str().unwrap().parse().unwrap();
    std::process::exit(i32::from(code));
}
