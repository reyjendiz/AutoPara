// A release build is a windowed app: no console window flashes up at logon.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    autopara_app::run();
}
