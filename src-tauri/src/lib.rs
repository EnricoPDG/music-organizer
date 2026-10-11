//! Music Organizer core library. The Tauri binary in `main.rs` is a thin wrapper around [`run`].

pub mod commands;
pub mod core;
pub mod db;
pub mod net;
pub mod os;
pub mod platforms;
pub mod tools;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Music Organizer");
}
