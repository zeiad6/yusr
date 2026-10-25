// A windowed app: without this, Windows opens a console window next to it in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    yusr_desktop_lib::run();
}
