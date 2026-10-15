// release ビルドで Windows のコンソールを出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nexora_lib::run()
}
