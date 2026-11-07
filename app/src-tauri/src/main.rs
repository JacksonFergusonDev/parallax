// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    erc_convert::amazon::run_child_if_asked();
    erc_app_lib::run();
}
