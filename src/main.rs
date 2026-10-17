use std::process::ExitCode;

fn main() -> ExitCode {
    match monkeytype_tui::runtime::main_entry(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("monkeytype-tui: {error}");
            ExitCode::FAILURE
        }
    }
}
