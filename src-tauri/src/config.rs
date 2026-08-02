use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[cfg(windows)]
use windows::Win32::{
    Globalization::GetUserDefaultLocaleName,
    System::Time::{GetDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION},
};

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

    /// Windows display device names that Lum must leave untouched.
    #[serde(default)]
    pub disabled_displays: Vec<String>,

    /// Dark/light theme switching.
    pub theme: ThemeConfig,

    /// Per-application pause list (process names, e.g. "photoshop.exe").
    pub pause_apps: Vec<String>,

    /// User-configurable global keyboard shortcuts.
    #[serde(default)]
    pub hotkeys: HotkeyConfig,

    /// Advanced quick-panel interaction preferences.
    #[serde(default)]
    pub developer: DeveloperConfig,
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

impl Default for Location {
    fn default() -> Self {
        detect_location().unwrap_or(Self {
            // Neutral fallback when the operating-system region cannot be read.
            latitude: 0.0,
            longitude: 0.0,
            timezone: None,
        })
    }
}

#[cfg(windows)]
fn detect_location() -> Option<Location> {
    const LOCALE_NAME_CAPACITY: usize = 85;
    let mut timezone = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // The return value describes DST state; the structure is populated for all
    // successful states, including TIME_ZONE_ID_UNKNOWN.
    unsafe { GetDynamicTimeZoneInformation(&mut timezone) };
    let timezone = utf16_array(&timezone.TimeZoneKeyName);

    let mut locale = [0u16; LOCALE_NAME_CAPACITY];
    let locale_len = unsafe { GetUserDefaultLocaleName(&mut locale) };
    let locale = (locale_len > 0).then(|| utf16_array(&locale))?;

    guess_location(&timezone, &locale)
}

#[cfg(not(windows))]
fn detect_location() -> Option<Location> {
    None
}

#[cfg(windows)]
fn utf16_array(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

/// Pick a representative city for solar calculations. Windows time-zone names
/// are used first; locale region handles zones shared by several countries.
fn guess_location(timezone: &str, locale: &str) -> Option<Location> {
    let region = locale
        .rsplit(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_uppercase();
    let coordinates = match (timezone, region.as_str()) {
        ("GMT Standard Time", "IE") => (53.3498, -6.2603),
        ("GMT Standard Time", _) => (51.5074, -0.1278),
        ("W. Europe Standard Time", "NL") => (52.3676, 4.9041),
        ("W. Europe Standard Time", "BE") => (50.8503, 4.3517),
        ("W. Europe Standard Time", "CH") => (46.9480, 7.4474),
        ("W. Europe Standard Time", "AT") => (48.2082, 16.3738),
        ("W. Europe Standard Time", _) => (52.5200, 13.4050),
        ("Romance Standard Time", "ES") => (40.4168, -3.7038),
        ("Romance Standard Time", _) => (48.8566, 2.3522),
        ("Central Europe Standard Time", "CZ") => (50.0755, 14.4378),
        ("Central Europe Standard Time", "HU") => (47.4979, 19.0402),
        ("Central Europe Standard Time", "SK") => (48.1486, 17.1077),
        ("Central Europe Standard Time", _) => (52.2297, 21.0122),
        ("E. Europe Standard Time", "FI") => (60.1699, 24.9384),
        ("E. Europe Standard Time", _) => (47.0105, 28.8638),
        ("FLE Standard Time", "EE") => (59.4370, 24.7536),
        ("FLE Standard Time", "LV") => (56.9496, 24.1052),
        ("FLE Standard Time", "LT") => (54.6872, 25.2797),
        ("FLE Standard Time", _) => (50.4501, 30.5234),
        ("Eastern Standard Time", "CA") => (43.6532, -79.3832),
        ("Eastern Standard Time", _) => (40.7128, -74.0060),
        ("Central Standard Time", "CA") => (49.8951, -97.1384),
        ("Central Standard Time", _) => (41.8781, -87.6298),
        ("Mountain Standard Time", "CA") => (51.0447, -114.0719),
        ("Mountain Standard Time", _) => (39.7392, -104.9903),
        ("Pacific Standard Time", "CA") => (49.2827, -123.1207),
        ("Pacific Standard Time", _) => (34.0522, -118.2437),
        ("Tokyo Standard Time", _) => (35.6762, 139.6503),
        ("Korea Standard Time", _) => (37.5665, 126.9780),
        ("China Standard Time", _) => (31.2304, 121.4737),
        ("India Standard Time", _) => (28.6139, 77.2090),
        ("AUS Eastern Standard Time", _) => (-33.8688, 151.2093),
        ("E. Australia Standard Time", _) => (-27.4698, 153.0251),
        ("New Zealand Standard Time", _) => (-41.2866, 174.7756),
        _ => match region.as_str() {
            "DE" => (52.5200, 13.4050),
            "FR" => (48.8566, 2.3522),
            "GB" => (51.5074, -0.1278),
            "US" => (39.8283, -98.5795),
            "CA" => (45.4215, -75.6972),
            "AU" => (-35.2809, 149.1300),
            "JP" => (35.6762, 139.6503),
            "IN" => (28.6139, 77.2090),
            "BR" => (-15.7939, -47.8828),
            "MX" => (19.4326, -99.1332),
            _ => return None,
        },
    };

    Some(Location {
        latitude: coordinates.0,
        longitude: coordinates.1,
        timezone: None,
    })
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
    pub show_quick_controls: bool,
    pub quick_controls_duration_sec: u32,
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
            show_quick_controls: true,
            quick_controls_duration_sec: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayClickBehavior {
    Immediate,
    ImmediateWithSettings,
    WindowsTimed,
}

impl Default for TrayClickBehavior {
    fn default() -> Self {
        Self::WindowsTimed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeveloperConfig {
    pub tray_click_behavior: TrayClickBehavior,
    pub close_on_focus_loss: bool,
    pub minimum_gamma_percent: u8,
}

impl Default for DeveloperConfig {
    fn default() -> Self {
        Self {
            tray_click_behavior: TrayClickBehavior::WindowsTimed,
            close_on_focus_loss: true,
            minimum_gamma_percent: 10,
        }
    }
}

impl DeveloperConfig {
    pub fn gamma_floor_percent(&self) -> u8 {
        self.minimum_gamma_percent.clamp(1, 100)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 3,
            location: Location::default(),
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
            disabled_displays: Vec::new(),
            theme: ThemeConfig {
                auto_switch: true,
                dark_offset_min: 0,
                light_offset_min: 0,
            },
            pause_apps: vec!["photoshop.exe".to_string(), "lightroom.exe".to_string()],
            hotkeys: HotkeyConfig::default(),
            developer: DeveloperConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{guess_location, DeveloperConfig};

    #[test]
    fn gamma_floor_defaults_to_ten_and_stays_in_range() {
        let mut developer = DeveloperConfig::default();
        assert_eq!(developer.gamma_floor_percent(), 10);
        developer.minimum_gamma_percent = 0;
        assert_eq!(developer.gamma_floor_percent(), 1);
        developer.minimum_gamma_percent = 255;
        assert_eq!(developer.gamma_floor_percent(), 100);
    }

    #[test]
    fn locale_disambiguates_shared_timezone() {
        let berlin = guess_location("W. Europe Standard Time", "de-DE").unwrap();
        let vienna = guess_location("W. Europe Standard Time", "de-AT").unwrap();
        assert_eq!((berlin.latitude, berlin.longitude), (52.5200, 13.4050));
        assert_eq!((vienna.latitude, vienna.longitude), (48.2082, 16.3738));
    }

    #[test]
    fn locale_is_used_when_timezone_is_unknown() {
        let location = guess_location("Unknown zone", "pt-BR").unwrap();
        assert_eq!(
            (location.latitude, location.longitude),
            (-15.7939, -47.8828)
        );
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
            Ok(json) => match serde_json::from_str::<Settings>(&json) {
                Ok(settings) => settings,
                Err(e) => {
                    eprintln!("[lum] Failed to parse settings: {e}. Using defaults.");
                    Self::default()
                }
            },
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
