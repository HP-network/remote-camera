#[cfg(not(windows))]
use crate::config::Config;

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    pub dry_run: bool,
    pub verbose: bool,
    pub allow_local: bool,
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::run;

#[cfg(not(windows))]
pub fn run(_config: Config, _options: RunOptions) {
    eprintln!("remote-camera is a Windows companion tool. Build it on Windows to enable the remote-control input backend.");
}
