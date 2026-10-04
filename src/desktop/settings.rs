use super::shortcut::{COLOR_PICKER_SHORTCUT, FULLSCREEN_SHORTCUT, SCREENSHOT_SHORTCUT, Shortcut};
use crate::{color_format::ColorFormat, dimensions::DimensionMode};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dimension_mode: DimensionMode,
    pub color_format: ColorFormat,
    pub sound_effects_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dimension_mode: DimensionMode::default(),
            color_format: ColorFormat::default(),
            sound_effects_enabled: true,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|directory| directory.join("Snipuno").join("settings.json"))
    }

    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let directory = path
            .parent()
            .filter(|directory| !directory.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(directory)?;
        fs::write(path, bytes)
    }
}

pub trait ShortcutBackend {
    async fn register(&mut self, shortcut: &Shortcut) -> Result<(), String>;
}

pub struct ShortcutService<B> {
    pub backend: B,
    pub active: Option<Shortcut>,
    pub fullscreen_active: Option<Shortcut>,
    pub color_active: Option<Shortcut>,
    pub error: Option<String>,
}

impl<B: ShortcutBackend> ShortcutService<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            active: None,
            fullscreen_active: None,
            color_active: None,
            error: None,
        }
    }

    pub async fn initialize(&mut self) {
        if cfg!(target_os = "linux") {
            return;
        }
        let mut errors = Vec::new();
        for (active, shortcut, name) in [
            (&mut self.active, SCREENSHOT_SHORTCUT, "Screenshot shortcut"),
            (
                &mut self.fullscreen_active,
                FULLSCREEN_SHORTCUT,
                "Full screen screenshot shortcut",
            ),
            (
                &mut self.color_active,
                COLOR_PICKER_SHORTCUT,
                "Color picker shortcut",
            ),
        ] {
            if active.is_none() {
                match self.backend.register(&shortcut).await {
                    Ok(()) => *active = Some(shortcut),
                    Err(error) => errors.push(format!("{name}: {error}")),
                }
            }
        }
        self.error = (!errors.is_empty()).then(|| errors.join("\n"));
    }
}
