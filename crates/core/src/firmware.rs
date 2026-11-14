//! Whether Kobo has newer software for a connected Kobo, as Kobo Utilities'
//! "Check for firmware updates" asks (janlarres/kobo-utilities, GPLv3,
//! features/firmware.py). Kobo's update service is asked with the model and
//! current version only; a placeholder stands in for the serial number.

use parallax_device_kobo::{FirmwareVersion, MAX_WRITE_FIRMWARE_MAJOR, VersionInfo};
use parallax_metadata::Fetch;
use serde::Serialize;
use serde_json::Value;

/// `on` to ask Kobo about new software when a Kobo connects.
pub const KOBO_FIRMWARE_CHECK: &str = "kobo_firmware_check";

const SERVICE: &str = "https://api.kobobooks.com/1.0/UpgradeCheck/Device";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirmwareUpdate {
    pub current: String,
    pub available: String,
    pub release_notes: Option<String>,
    /// Newer than any version Parallax changes, so it would stop making
    /// changes to this Kobo after the update.
    pub beyond_support: bool,
}

/// The address asked for `version`'s Kobo.
pub fn check_url(version: &VersionInfo) -> String {
    format!(
        "{SERVICE}/{model}/kobo/{firmware}/N",
        model = version.model_id,
        firmware = version.firmware
    )
}

/// The update in Kobo's answer, if it's newer than `current`.
pub fn parse(body: &str, current: &FirmwareVersion) -> Option<FirmwareUpdate> {
    let json: Value = serde_json::from_str(body).ok()?;
    let url = json.get("UpgradeURL")?.as_str()?;
    let file = url.rsplit('/').next()?;
    let available = file.strip_prefix("kobo-update-")?.strip_suffix(".zip")?;
    let available = FirmwareVersion::parse(available)?;
    if &available <= current {
        return None;
    }
    Some(FirmwareUpdate {
        current: current.to_string(),
        beyond_support: available.0.first().copied().unwrap_or(0) > MAX_WRITE_FIRMWARE_MAJOR,
        available: available.to_string(),
        release_notes: json
            .get("ReleaseNoteURL")
            .and_then(Value::as_str)
            .filter(|u| u.starts_with("https://"))
            .map(str::to_owned),
    })
}

/// Asks Kobo whether there's newer software for this Kobo. Tolino readers
/// update through Tolino, so they're never asked about.
pub fn check(fetch: &dyn Fetch, version: &VersionInfo) -> Result<Option<FirmwareUpdate>, String> {
    if version.is_tolino() || version.model_id.is_empty() {
        return Ok(None);
    }
    let body = fetch
        .get(&check_url(version), &[])
        .map_err(|e| e.to_string())?;
    Ok(parse(&body, &version.firmware))
}

/// [`check`] over the internet.
pub fn check_online(version: &VersionInfo) -> Result<Option<FirmwareUpdate>, String> {
    let fetcher = parallax_metadata::HttpFetcher::new().map_err(|e| e.to_string())?;
    check(&fetcher, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version() -> VersionInfo {
        VersionInfo::parse(
            "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388",
        )
        .unwrap()
    }

    #[test]
    fn asks_without_the_serial_number() {
        assert_eq!(
            check_url(&version()),
            "https://api.kobobooks.com/1.0/UpgradeCheck/Device/00000000-0000-0000-0000-000000000388/kobo/4.38.23697/N"
        );
    }

    #[test]
    fn reads_kobos_answer() {
        let current = version().firmware;
        // Recorded from Kobo's service on 14 November 2026.
        let newer = r#"{"UpgradeType":2,"UpgradeURL":"https://ereaderfiles.kobo.com/firmwares/kobo9/Oct2026/kobo-update-4.38.23852.zip","ReleaseNoteURL":"https://api.kobobooks.com/1.0/ReleaseNotes/190","Data":null}"#;
        assert_eq!(
            parse(newer, &current),
            Some(FirmwareUpdate {
                current: "4.38.23697".into(),
                available: "4.38.23852".into(),
                release_notes: Some("https://api.kobobooks.com/1.0/ReleaseNotes/190".into()),
                beyond_support: false,
            })
        );
        let none = r#"{"UpgradeType":0,"UpgradeURL":null,"ReleaseNoteURL":null,"Data":null}"#;
        assert_eq!(parse(none, &current), None);
        let five = r#"{"UpgradeURL":"https://x/kobo-update-5.0.1.zip"}"#;
        assert!(parse(five, &current).unwrap().beyond_support);
        assert_eq!(parse("not json", &current), None);
    }
}
