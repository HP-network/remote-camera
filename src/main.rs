mod config;
#[cfg_attr(not(windows), allow(dead_code))]
mod engine;
#[cfg_attr(not(windows), allow(dead_code))]
mod filter;
mod geometry;
mod platform;

use std::path::PathBuf;

use config::{config_from_path, Config};
use platform::RunOptions;

fn print_help() {
    println!("Remote Camera {}", env!("CARGO_PKG_VERSION"));
    println!("Windows companion for stable remote-control mouse input");
    println!();
    println!("Usage: remote-camera [options]");
    println!("  --config <path>  use a specific config file");
    println!("  --print-config   print the effective configuration and exit");
    println!("  --dry-run        observe and filter input without injecting movement");
    println!("  --verbose        print target and runtime transitions");
    println!("  --no-rdp         compatibility alias for session_mode=any");
    println!("  -h, --help       show this help");
    println!("  -V, --version    show the version");
    println!();
    println!("F8 toggles the stabilizer. F9 exits. target_scope=desktop is the default; target_scope=minecraft limits handling to Minecraft.");
}

fn main() {
    let mut config_path: Option<PathBuf> = None;
    let mut options = RunOptions::default();
    let mut print_config = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-V" | "--version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--config" => {
                let Some(path) = args.next() else {
                    eprintln!("--config 需要路径");
                    std::process::exit(2);
                };
                config_path = Some(PathBuf::from(path));
            }
            "--print-config" => print_config = true,
            "--dry-run" => options.dry_run = true,
            "--verbose" => options.verbose = true,
            "--no-rdp" => options.allow_local = true,
            unknown => {
                eprintln!("未知参数: {unknown}\n使用 --help 查看参数");
                std::process::exit(2);
            }
        }
    }

    let mut config = config_path
        .map(config_from_path)
        .unwrap_or_else(Config::load);
    if options.allow_local {
        config.require_rdp = false;
        config.session_mode = "any".to_owned();
    }
    if print_config {
        print!("{}", config.to_text());
        return;
    }
    platform::run(config, options);
}
