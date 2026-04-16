pub mod window_controller;
pub mod window_manager;

#[cfg(target_os = "windows")]
pub mod windows;

pub use window_controller::{WindowController, WindowOps};
pub use window_manager::WindowManager;

pub type MainWindowController = WindowController;
