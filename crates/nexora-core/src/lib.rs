//! Nexora Launcher のビジネスロジック。Tauri には依存しない。

pub mod paths;

pub mod content;

pub mod error;

pub mod db;
pub mod settings;

pub mod task;

pub mod app_info;

pub mod install;
pub mod instance;
pub mod meta;
pub mod net;

pub mod java;

pub mod auth;
pub mod launch;
pub mod launcher;

pub mod sync;

pub mod system;
