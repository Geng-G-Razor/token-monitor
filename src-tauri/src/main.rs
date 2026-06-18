// Prevents an additional console window on Windows; no-op elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    token_monitor_lib::run()
}
