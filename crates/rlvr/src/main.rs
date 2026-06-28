use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    match fractal_rlvr::run_argv(&argv) {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
