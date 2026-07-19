use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Top-level settings schema. Persisted as JSON in %AppData%\Lum\settings.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Schema version for forward-compatible migration.
    pub version: u32,

    /// Which rendering backend to use.
    pub engine: EngineKind,

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    GammaRamps,
    NightLight,
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
    /// Daytime brightness percentage (0-100).
    pub day_percent: u8,
    /// Nighttime brightness percentage (0-100). Floor to never go fully dark.
    pub night_percent: u8,
    /// Whether brightness follows the same curve as color or a separate one.
    pub linked_to_color: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Automatically switch Windows theme with the sun schedule.
    pub auto_switch: bool,
    /// Use dark theme at night (true) or during day (false).
    pub dark_at_night: bool,
    /// Minutes from midnight to switch TO dark theme (0-1440). Draggable on curve UI.
    #[serde(default = "default_dark_at")]
    pub dark_at_min: u32,
    /// Minutes from midnight to switch TO light theme (0-1440). Draggable on curve UI.
    #[serde(default = "default_light_at")]
    pub light_at_min: u32,
}

fn default_dark_at() -> u32 { 1200 } // 20:00
fn default_light_at() -> u32 { 420 } // 07:00

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            engine: EngineKind::GammaRamps,
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
                day_percent: 100,
                night_percent: 70,
                linked_to_color: true,
            },
            theme: ThemeConfig {
                auto_switch: true,
                dark_at_night: true,
                dark_at_min: 1200,
                light_at_min: 420,
            },
            pause_apps: vec![
                "photoshop.exe".to_string(),
                "lightroom.exe".to_string(),
            ],
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
