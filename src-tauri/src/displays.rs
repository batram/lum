//! Hardware identity of connected displays.
//!
//! Windows numbers logical displays (`\\.\DISPLAY1`) by connection order, so the
//! same monitor can change name between boots or after a replug. This module
//! maps each logical display to the monitor's EDID model name and a stable id
//! (`PNPID-SERIAL`, e.g. `GSM5BB4-106MATW1D937`) that settings are keyed by.

use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY};

/// One connected monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayIdentity {
    /// Windows logical display name (for example `\\.\DISPLAY1`).
    pub device_name: String,
    /// Stable id: `PNPID-SERIAL`, or `PNPID@INSTANCE` (tied to the port) when
    /// the monitor reports no serial.
    pub id: String,
    /// EDID model name (for example `LG ULTRAGEAR`); may be empty.
    pub name: String,
    /// EDID serial number; may be empty.
    pub serial: String,
}

static IDENTITIES: Mutex<Vec<DisplayIdentity>> = Mutex::new(Vec::new());

/// Re-read display identities from Windows. Keeps the previous list if the
/// query fails or comes back empty during display topology churn.
pub fn refresh() -> Vec<DisplayIdentity> {
    let found = query_identities();
    let mut cached = IDENTITIES.lock().unwrap();
    if !found.is_empty() {
        *cached = found;
    }
    cached.clone()
}

/// Identities from the last refresh.
pub fn identities() -> Vec<DisplayIdentity> {
    IDENTITIES.lock().unwrap().clone()
}

/// Whether a settings key refers to the logical display `device_name`.
/// Keys are stable ids; legacy keys are logical display names.
pub fn matches(key: &str, device_name: &str) -> bool {
    key.eq_ignore_ascii_case(device_name)
        || IDENTITIES.lock().unwrap().iter().any(|identity| {
            identity.device_name.eq_ignore_ascii_case(device_name)
                && identity.id.eq_ignore_ascii_case(key)
        })
}

/// Stable id for a settings key that may still be a legacy logical display name.
pub fn stable_key(key: &str, identities: &[DisplayIdentity]) -> String {
    identities
        .iter()
        .find(|identity| identity.device_name.eq_ignore_ascii_case(key))
        .map(|identity| identity.id.clone())
        .unwrap_or_else(|| key.to_string())
}

fn query_identities() -> Vec<DisplayIdentity> {
    let mut path_count = 0u32;
    let mut mode_count = 0u32;
    if unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    } != ERROR_SUCCESS
    {
        return Vec::new();
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    if unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    } != ERROR_SUCCESS
    {
        return Vec::new();
    }
    paths.truncate(path_count as usize);

    paths
        .iter()
        .filter_map(|path| {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            source.header.adapterId = path.sourceInfo.adapterId;
            source.header.id = path.sourceInfo.id;
            if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
                return None;
            }

            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            target.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            target.header.adapterId = path.targetInfo.adapterId;
            target.header.id = path.targetInfo.id;
            if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != 0 {
                return None;
            }

            let device_name = utf16(&source.viewGdiDeviceName);
            let device_path = utf16(&target.monitorDevicePath);
            let (pnp_id, instance) = parse_device_path(&device_path)?;
            let edid = read_edid(&pnp_id, &instance).unwrap_or_default();
            let serial = edid_serial(&edid);
            let mut name = utf16(&target.monitorFriendlyDeviceName);
            if name.is_empty() {
                name = edid_descriptor(&edid, 0xFC).unwrap_or_default();
            }
            Some(DisplayIdentity {
                device_name,
                id: stable_id(&pnp_id, &serial, &instance),
                name,
                serial,
            })
        })
        .collect()
}

fn utf16(value: &[u16]) -> String {
    let len = value.iter().position(|&c| c == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..len])
}

/// Split `\\?\DISPLAY#GSM5BB4#5&1079cd01&1&UID176387#{guid}` into the PnP id
/// and the device instance.
fn parse_device_path(path: &str) -> Option<(String, String)> {
    let mut parts = path.trim_start_matches(r"\\?\").split('#');
    if !parts.next()?.eq_ignore_ascii_case("DISPLAY") {
        return None;
    }
    let pnp_id = parts.next()?.to_ascii_uppercase();
    let instance = parts.next()?.to_string();
    (!pnp_id.is_empty() && !instance.is_empty()).then_some((pnp_id, instance))
}

fn stable_id(pnp_id: &str, serial: &str, instance: &str) -> String {
    if serial.is_empty() {
        format!("{pnp_id}@{}", instance.to_ascii_uppercase())
    } else {
        format!("{pnp_id}-{serial}")
    }
}

fn read_edid(pnp_id: &str, instance: &str) -> Option<Vec<u8>> {
    let key: Vec<u16> =
        format!(r"SYSTEM\CurrentControlSet\Enum\DISPLAY\{pnp_id}\{instance}\Device Parameters")
            .encode_utf16()
            .chain(Some(0))
            .collect();
    let value: Vec<u16> = "EDID".encode_utf16().chain(Some(0)).collect();
    let mut data = vec![0u8; 512];
    let mut size = data.len() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if result != ERROR_SUCCESS {
        return None;
    }
    data.truncate(size as usize);
    Some(data)
}

/// Text of an EDID display descriptor (0xFF serial, 0xFC model name).
fn edid_descriptor(edid: &[u8], tag: u8) -> Option<String> {
    (0..4).find_map(|index| {
        let block = edid.get(54 + index * 18..72 + index * 18)?;
        if block[..3] != [0, 0, 0] || block[3] != tag {
            return None;
        }
        let text: String = block[5..]
            .iter()
            .take_while(|&&byte| byte != 0x0A && byte != 0)
            .map(|&byte| byte as char)
            .collect();
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    })
}

/// Serial from the 0xFF descriptor, falling back to the numeric header serial.
fn edid_serial(edid: &[u8]) -> String {
    if let Some(serial) = edid_descriptor(edid, 0xFF) {
        return serial;
    }
    match edid.get(12..16) {
        Some(bytes) => match u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) {
            0 | 0x0101_0101 => String::new(),
            serial => serial.to_string(),
        },
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edid_with(descriptors: &[(u8, &str)], header_serial: u32) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[12..16].copy_from_slice(&header_serial.to_le_bytes());
        for (index, (tag, text)) in descriptors.iter().enumerate() {
            let start = 54 + index * 18;
            edid[start + 3] = *tag;
            let mut payload = text.as_bytes().to_vec();
            payload.push(0x0A);
            payload.resize(13, 0x20);
            edid[start + 5..start + 18].copy_from_slice(&payload);
        }
        edid
    }

    #[test]
    fn reads_model_name_and_serial_descriptors() {
        let edid = edid_with(&[(0xFF, "106MATW1D937"), (0xFC, "LG ULTRAGEAR")], 0);
        assert_eq!(edid_serial(&edid), "106MATW1D937");
        assert_eq!(edid_descriptor(&edid, 0xFC).as_deref(), Some("LG ULTRAGEAR"));
    }

    #[test]
    fn falls_back_to_header_serial_and_ignores_placeholders() {
        assert_eq!(edid_serial(&edid_with(&[], 12345)), "12345");
        assert_eq!(edid_serial(&edid_with(&[], 0x0101_0101)), "");
        assert_eq!(edid_serial(&[]), "");
    }

    #[test]
    fn parses_device_path_and_builds_ids() {
        let (pnp, instance) = parse_device_path(
            r"\\?\DISPLAY#GSM5BB4#5&1079cd01&1&UID176387#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}",
        )
        .unwrap();
        assert_eq!(pnp, "GSM5BB4");
        assert_eq!(instance, "5&1079cd01&1&UID176387");
        assert_eq!(stable_id(&pnp, "106MATW1D937", &instance), "GSM5BB4-106MATW1D937");
        assert_eq!(stable_id(&pnp, "", &instance), "GSM5BB4@5&1079CD01&1&UID176387");
        assert!(parse_device_path(r"\\?\USB#VID_1234").is_none());
    }

    #[test]
    fn legacy_keys_become_stable_ids_when_connected() {
        let identities = vec![DisplayIdentity {
            device_name: r"\\.\DISPLAY1".into(),
            id: "GSM5BB4-106MATW1D937".into(),
            name: "LG ULTRAGEAR".into(),
            serial: "106MATW1D937".into(),
        }];
        assert_eq!(stable_key(r"\\.\display1", &identities), "GSM5BB4-106MATW1D937");
        assert_eq!(stable_key(r"\\.\DISPLAY3", &identities), r"\\.\DISPLAY3");
        assert_eq!(stable_key("GSM5BB4-106MATW1D937", &identities), "GSM5BB4-106MATW1D937");
    }
}
