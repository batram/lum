use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Top-level settings schema. Persisted as JSON in %AppData%\Lum\settings.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Schema version for forward-compatible migration.
    pub version: u32,

    /// User's geographic location for sun calculations.
    pub location: Location,

    /// Fade timing configuration.
    pub fade: FadeConfig,

    /// Color temperature endpoints (Kelvin).
    pub color: ColorConfig,

    /// Brightness configuration.
    pub brightness: BrightnessConfig,

    /// Dark/light theme switching.
    pub theme: ThemeConfig,

    /// Per-application pause list (process names, e.g. "photoshop.exe").
    pub pause_apps: Vec<String>,

    /// User-configurable global keyboard shortcuts.
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    /// IANA timezone name (e.g. "America/New_York"). Defaults to system local.
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FadeConfig {
    /// Minutes before sunset to begin evening fade.
    pub evening_offset_min: i32,
    /// Minutes after sunrise to finish morning fade.
    pub morning_offset_min: i32,
    /// Duration of the fade transition in minutes.
    pub fade_duration_min: u32,
    /// Use civil twilight (-6°) instead of fixed offsets.
    pub use_civil_twilight: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    /// Daytime color temperature (Kelvin). 6500 = neutral daylight.
    pub day_temp_k: u32,
    /// Nighttime color temperature (Kelvin). 3400 = warm.
    pub night_temp_k: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrightnessConfig {
    pub hardware_day_percent: u8,
    pub hardware_night_percent: u8,
    pub overlay_day_percent: u8,
    pub overlay_night_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub auto_switch: bool,
    /// Offset from sunset/civil dusk, in minutes.
    pub dark_offset_min: i32,
    /// Offset from sunrise/civil dawn, in minutes.
    pub light_offset_min: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    pub toggle_pause: String,
    pub brighter: String,
    pub darker: String,
    pub toggle_theme: String,
    pub toggle_day_night: String,
    pub boost: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle_pause: "Alt+Pause".into(),
            brighter: "Alt+Up".into(),
            darker: "Alt+Down".into(),
            toggle_theme: "Alt+F5".into(),
            toggle_day_night: "Alt+F6".into(),
            boost: "Alt+F7".into(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 2,
            location: Location {
                // Default: New York City
                latitude: 40.7128,
                longitude: -74.0060,
                timezone: None,
            },
            fade: FadeConfig {
                evening_offset_min: 0,
                morning_offset_min: 0,
                fade_duration_min: 60,
                use_civil_twilight: false,
            },
            color: ColorConfig {
                day_temp_k: 6500,
                night_temp_k: 3400,
            },
            brightness: BrightnessConfig {
                hardware_day_percent: 100,
                hardware_night_percent: 70,
                overlay_day_percent: 100,
                overlay_night_percent: 100,
            },
            theme: ThemeConfig {
                auto_switch: true,
                dark_offset_min: 0,
                light_offset_min: 0,
            },
            pause_apps: vec![
                "photoshop.exe".to_string(),
                "lightroom.exe".to_string(),
            ],
            hotkeys: HotkeyConfig::default(),
        }
    }
}

impl Settings {
    /// Path to the settings file: %AppData%\Lum\settings.json
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("Lum").join("settings.json")
    }

    /// Load settings from disk. Returns defaults if file doesn't exist or is invalid.
    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(json) => {
                match serde_json::from_str::<Settings>(&json) {
                    Ok(settings) => settings,
                    Err(e) => {
                        eprintln!("[lum] Failed to parse settings: {e}. Using defaults.");
                        Self::default()
                    }
                }
            }
            Err(_) => {
                // First run — no settings file yet
                let defaults = Self::default();
                defaults.save();
                defaults
            }
        }
    }

    /// Persist settings to disk. Creates the Lum directory if needed.
    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    eprintln!("[lum] Failed to write settings: {e}");
                }
            }
            Err(e) => eprintln!("[lum] Failed to serialize settings: {e}"),
        }
    }
}
