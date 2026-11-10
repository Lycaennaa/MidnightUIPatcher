use std::process::ExitCode;

mod cli;

fn main() -> ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        #[cfg(feature = "desktop")]
        {
            midnight_ui_patcher::ui::run(None);
            return ExitCode::SUCCESS;
        }
        #[cfg(not(feature = "desktop"))]
        {
            return cli::run([std::ffi::OsString::from("--help")]);
        }
    }
    cli::run(args)
}
