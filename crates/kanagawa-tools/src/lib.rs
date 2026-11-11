//! kanagawa-tools — Shared system core for Kanagawa Waybar providers and desktop daemons.

pub mod sys;
pub mod ipc;
pub mod format;

pub use sys::*;
pub use ipc::*;
pub use format::*;
