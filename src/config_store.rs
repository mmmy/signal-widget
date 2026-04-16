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
        let mut draft = inner.config.clone();
        update(&mut draft.ui);
        draft.save_to(Path::new(&inner.path))?;
        inner.config = draft;
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "signal-desk-widget-plan-{name}-{unique}-{}.yaml",
            std::process::id()
        ))
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
        let reloaded: crate::config::AppConfig =
            serde_yaml::from_str(&raw).expect("reload persisted yaml");
        assert_eq!(reloaded.ui.widget.x, 180.0);
        assert_eq!(reloaded.ui.widget.y, 220.0);
    }

    #[test]
    fn update_ui_keeps_snapshot_unchanged_when_persistence_fails() {
        let path = std::env::temp_dir().join(format!(
            "signal-desk-widget-plan-update-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir(&path).expect("create temp dir");

        let store = ConfigStore::new_for_test(crate::config::AppConfig::default(), path.clone());
        let before = store.snapshot();

        let result = store.update_ui(|ui| {
            ui.widget.x = 180.0;
            ui.widget.y = 220.0;
        });

        assert!(result.is_err());
        assert_eq!(store.snapshot().ui.widget.x, before.ui.widget.x);
        assert_eq!(store.snapshot().ui.widget.y, before.ui.widget.y);

        std::fs::remove_dir_all(path).expect("clean up temp dir");
    }
}
