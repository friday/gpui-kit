#![cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux;

pub use linux::{activate_next_window_with_token, current_platform, linux_platform};
