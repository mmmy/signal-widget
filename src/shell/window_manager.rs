use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::core::contract::{ShellCommand, WindowId};

use super::window_controller::WindowOps;

#[derive(Default)]
pub struct WindowManager {
    windows: RwLock<HashMap<WindowId, Arc<dyn WindowOps>>>,
}

impl WindowManager {
    pub fn register(&self, id: WindowId, window: Arc<dyn WindowOps>) {
        self.windows.write().insert(id, window);
    }

    pub fn apply(&self, command: &ShellCommand) {
        match command {
            ShellCommand::ShowWindow(id) => {
                if let Some(window) = self.windows.read().get(id) {
                    window.show();
                }
            }
            ShellCommand::HideWindow(id) => {
                if let Some(window) = self.windows.read().get(id) {
                    window.hide();
                }
            }
            ShellCommand::FocusWindow(id) => {
                if let Some(window) = self.windows.read().get(id) {
                    window.focus();
                }
            }
            ShellCommand::ExitProcess => {
                if let Some(window) = self.windows.read().get(&WindowId::Main) {
                    window.request_close();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingWindow {
        actions: Arc<Mutex<Vec<&'static str>>>,
    }

    impl WindowOps for RecordingWindow {
        fn show(&self) {
            self.actions.lock().unwrap().push("show");
        }

        fn hide(&self) {
            self.actions.lock().unwrap().push("hide");
        }

        fn focus(&self) {
            self.actions.lock().unwrap().push("focus");
        }

        fn request_close(&self) {
            self.actions.lock().unwrap().push("close");
        }
    }

    #[test]
    fn hide_widget_only_touches_widget_window() {
        let main = Arc::new(RecordingWindow::default());
        let widget = Arc::new(RecordingWindow::default());
        let manager = WindowManager::default();
        manager.register(WindowId::Main, main.clone());
        manager.register(WindowId::Widget, widget.clone());

        manager.apply(&ShellCommand::HideWindow(WindowId::Widget));

        assert_eq!(*main.actions.lock().unwrap(), Vec::<&'static str>::new());
        assert_eq!(*widget.actions.lock().unwrap(), vec!["hide"]);
    }

    #[test]
    fn exit_process_requests_main_close() {
        let main = Arc::new(RecordingWindow::default());
        let manager = WindowManager::default();
        manager.register(WindowId::Main, main.clone());

        manager.apply(&ShellCommand::ExitProcess);

        assert_eq!(*main.actions.lock().unwrap(), vec!["close"]);
    }
}
