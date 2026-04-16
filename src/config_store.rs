use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use parking_lot::Mutex;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct ConfigStore {
    inner: Arc<Mutex<ConfigStoreInner>>,
}

struct ConfigStoreInner {
    config: AppConfig,
    path: PathBuf,
}

impl ConfigStore {
    pub fn load() -> Result<Self> {
        let (config, path) = AppConfig::load_or_create()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(ConfigStoreInner { config, path })),
        })
    }

    pub fn snapshot(&self) -> AppConfig {
        self.inner.lock().config.clone()
    }

    pub fn path(&self) -> PathBuf {
        self.inner.lock().path.clone()
    }

    pub fn update_ui<F>(&self, update: F) -> Result<AppConfig>
    where
        F: FnOnce(&mut crate::config::UiConfig),
    {
        let mut inner = self.inner.lock();
        update(&mut inner.config.ui);
        inner.config.save_to(Path::new(&inner.path))?;
        Ok(inner.config.clone())
    }

    #[cfg(test)]
    pub fn new_for_test(config: AppConfig, path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ConfigStoreInner { config, path })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigStore;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("signal-desk-widget-plan-{name}.yaml"))
    }

    #[test]
    fn widget_defaults_are_visible_and_have_expected_size() {
        let cfg = crate::config::UiConfig::default();
        assert!(cfg.widget.visible);
        assert_eq!(cfg.widget.size, 56.0);
        assert_eq!(cfg.widget.x, 32.0);
        assert_eq!(cfg.widget.y, 32.0);
    }

    #[test]
    fn update_ui_persists_widget_position() {
        let path = temp_path("widget-position");
        let store = ConfigStore::new_for_test(crate::config::AppConfig::default(), path.clone());

        let updated = store
            .update_ui(|ui| {
                ui.widget.x = 180.0;
                ui.widget.y = 220.0;
            })
            .expect("update ui");

        assert_eq!(updated.ui.widget.x, 180.0);
        assert_eq!(updated.ui.widget.y, 220.0);

        let raw = std::fs::read_to_string(path).expect("read persisted yaml");
        assert!(raw.contains("x: 180.0"));
        assert!(raw.contains("y: 220.0"));
    }
}
