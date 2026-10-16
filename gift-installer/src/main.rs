#![cfg_attr(windows, windows_subsystem = "windows")]

mod motion;
#[cfg(windows)]
mod native;

fn main() {
    #[cfg(windows)]
    if let Err(error) = native::run() {
        native::show_error(&error);
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    println!("The gift installer runs on Windows 11 x64. Browser preview: gift-installer/preview.");
}
