#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(index) = args.iter().position(|arg| arg == "--cli") {
        let mut cli_args = args;
        cli_args.remove(index);
        std::process::exit(moyumax_desktop_lib::run_cli(cli_args));
    }
    moyumax_desktop_lib::run();
}
