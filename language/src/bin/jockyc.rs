use std::ffi::OsString;
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<OsString>>();
    let outcome = jocky_language::cli::run(
        arguments,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    );
    ExitCode::from(outcome.exit_code())
}
