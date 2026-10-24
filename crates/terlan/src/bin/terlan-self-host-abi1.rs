use std::env;
use std::fs;
use std::process::ExitCode;

use terlan::self_host_backend::decode_backend_program;

fn main() -> ExitCode {
    let paths = env::args().skip(1).collect::<Vec<_>>();
    if paths.is_empty() {
        eprintln!("usage: terlan-self-host-abi1 ARTIFACT.abi1.json [...]");
        return ExitCode::from(2);
    }

    for path in &paths {
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("cannot read {path}: {error}");
                return ExitCode::from(1);
            }
        };
        if let Err(error) = decode_backend_program(&source) {
            eprintln!("cannot admit {path}: {error:?}");
            return ExitCode::from(1);
        }
    }

    println!("admitted {} typed ABI-1 artifacts", paths.len());
    ExitCode::SUCCESS
}
