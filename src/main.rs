#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native;

fn main() {
    #[cfg(windows)]
    if let Err(error) = native::run() {
        if std::env::args().any(|arg| arg == "--quiet") {
            eprintln!("{error}");
        } else {
            native::fatal(&error);
        }
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    eprintln!(
        "Orbit is a Windows application. Build with --target x86_64-pc-windows-msvc on Windows."
    );
}
