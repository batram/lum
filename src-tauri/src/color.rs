//! Color temperature (Kelvin) to RGB conversion for gamma ramp generation.
//! Based on Tanner Helland's approximation algorithm.

/// RGB multiplier values (0.0–1.0) representing a color temperature.
#[derive(Debug, Clone, Copy)]
pub struct RgbMultiplier {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// Convert a color temperature in Kelvin (1000–40000) to RGB multipliers.
///
/// Returns values normalized so that 6500K ≈ (1.0, 1.0, 1.0) neutral white.
pub fn kelvin_to_rgb(kelvin: u32) -> RgbMultiplier {
    let temp = (kelvin.clamp(1000, 40000) as f64) / 100.0;

    // Red
    let r = if temp <= 66.0 {
        255.0
    } else {
        let x = temp - 60.0;
        329.698727446 * x.powf(-0.1332047592)
    };

    // Green
    let g = if temp <= 66.0 {
        99.4708025861 * temp.ln() - 161.1195681661
    } else {
        let x = temp - 60.0;
        288.1221695283 * x.powf(-0.0755148492)
    };

    // Blue
    let b = if temp >= 66.0 {
        255.0
    } else if temp <= 19.0 {
        0.0
    } else {
        let x = temp - 10.0;
        138.5177312231 * x.ln() - 305.0447927307
    };

    // Normalize relative to 6500K (reference white)
    let ref_white = raw_rgb_at_6500();
    RgbMultiplier {
        r: (r.clamp(0.0, 255.0) / ref_white.0).clamp(0.0, 1.0),
        g: (g.clamp(0.0, 255.0) / ref_white.1).clamp(0.0, 1.0),
        b: (b.clamp(0.0, 255.0) / ref_white.2).clamp(0.0, 1.0),
    }
}

/// Interpolate between two color temperatures based on intensity (0=day, 1=night).
pub fn lerp_kelvin(day_k: u32, night_k: u32, intensity: f64) -> u32 {
    let t = intensity.clamp(0.0, 1.0);
    (day_k as f64 + (night_k as f64 - day_k as f64) * t).round() as u32
}

/// Raw RGB values at 6500K for normalization reference.
fn raw_rgb_at_6500() -> (f64, f64, f64) {
    // At 6500K: temp = 65.0
    let temp: f64 = 65.0;
    let r: f64 = 255.0; // temp <= 66
    let g: f64 = 99.4708025861 * temp.ln() - 161.1195681661;
    let b: f64 = 138.5177312231 * (temp - 10.0).ln() - 305.0447927307;
    (r, g.clamp(0.0, 255.0), b.clamp(0.0, 255.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_6500k_is_neutral() {
        let rgb = kelvin_to_rgb(6500);
        // At reference white, all channels should be ~1.0
        assert!((rgb.r - 1.0).abs() < 0.02);
        assert!((rgb.g - 1.0).abs() < 0.02);
        assert!((rgb.b - 1.0).abs() < 0.02);
    }

    #[test]
    fn test_warm_temp_reduces_blue() {
        let rgb = kelvin_to_rgb(3400);
        // Warm temp: blue should be significantly reduced
        assert!(rgb.b < 0.7);
        // Red stays high
        assert!(rgb.r > 0.95);
    }

    #[test]
    fn test_lerp() {
        assert_eq!(lerp_kelvin(6500, 3400, 0.0), 6500);
        assert_eq!(lerp_kelvin(6500, 3400, 1.0), 3400);
        assert_eq!(lerp_kelvin(6500, 3400, 0.5), 4950);
    }
}
