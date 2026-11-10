//! Parsing of `.kobo/version`, a single CSV line such as
//! `N418273049649,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388`.
//! Field layout follows calibre's `KOBO.device_version_info`: serial first,
//! firmware third, model id last.

use std::fmt;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub serial: String,
    pub firmware: FirmwareVersion,
    /// e.g. `00000000-0000-0000-0000-000000000388`.
    pub model_id: String,
}

impl VersionInfo {
    pub fn parse(contents: &str) -> Result<Self> {
        let fields: Vec<&str> = contents.trim().split(',').map(str::trim).collect();
        let bad = || Error::BadVersionFile(contents.trim().to_owned());
        if fields.len() < 3 || fields[0].is_empty() {
            return Err(bad());
        }
        Ok(Self {
            serial: fields[0].to_owned(),
            firmware: FirmwareVersion::parse(fields[2]).ok_or_else(bad)?,
            model_id: fields.last().copied().unwrap_or_default().to_owned(),
        })
    }

    /// The model id's significant digits (`388`); so far only the last three
    /// have ever been used.
    fn short_model_id(&self) -> &str {
        let id = self.model_id.rsplit('-').next().unwrap_or_default();
        id.trim_start_matches('0')
    }

    /// Model name for the known model ids (`screen::MODELS`); others fall
    /// back to a generic name. A device report (`parallax-cli kobo report`) is how new
    /// ones get added.
    pub fn model_name(&self) -> &'static str {
        crate::screen::find(self.short_model_id()).map_or("Kobo eReader", |m| m.name)
    }

    /// The screen comics are prepared for; `mount` is used only for a model
    /// that isn't known.
    pub fn screen(&self, mount: &std::path::Path) -> parallax_device::Screen {
        crate::screen::screen(self.short_model_id(), mount)
    }

    /// Whether this model has been through the hardware checks.
    pub fn is_tested(&self) -> bool {
        crate::support::TESTED_MODELS.contains(&self.short_model_id())
    }

    /// Tolino readers run Kobo's software but store data differently
    /// (calibre `KOBOTOUCH.isTolinoDevice`).
    pub fn is_tolino(&self) -> bool {
        matches!(self.short_model_id(), "690" | "691" | "693")
    }
}

/// Dotted firmware version, compared numerically (4.38.23697 > 4.9.0).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FirmwareVersion(pub Vec<u32>);

impl FirmwareVersion {
    pub fn parse(s: &str) -> Option<Self> {
        let parts = s
            .split('.')
            .map(|p| p.parse().ok())
            .collect::<Option<Vec<u32>>>()?;
        (!parts.is_empty()).then_some(Self(parts))
    }
}

impl fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(u32::to_string).collect();
        f.write_str(&parts.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRA2: &str =
        "N418273049649,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388\n";

    #[test]
    fn parses_libra2_version_file() {
        let v = VersionInfo::parse(LIBRA2).unwrap();
        assert_eq!(v.serial, "N418273049649");
        assert_eq!(v.firmware.to_string(), "4.38.23697");
        assert_eq!(v.model_name(), "Kobo Libra 2");
    }

    #[test]
    fn firmware_compares_numerically() {
        let a = FirmwareVersion::parse("4.38.23697").unwrap();
        let b = FirmwareVersion::parse("4.9.0").unwrap();
        assert!(a > b);
    }

    #[test]
    fn unknown_model_falls_back() {
        let v = VersionInfo::parse("N1,x,4.1.0,x,x,00000000-0000-0000-0000-000000000999").unwrap();
        assert_eq!(v.model_name(), "Kobo eReader");
    }

    #[test]
    fn recognises_tolino_readers() {
        let v = VersionInfo::parse("N1,x,5.6.0,x,x,00000000-0000-0000-0000-000000000693").unwrap();
        assert_eq!(v.model_name(), "Tolino Shine Color");
        assert!(v.is_tolino());
        assert!(!VersionInfo::parse(LIBRA2).unwrap().is_tolino());
    }

    #[test]
    fn knows_which_models_have_been_tested() {
        assert!(VersionInfo::parse(LIBRA2).unwrap().is_tested());
        let clara =
            VersionInfo::parse("N1,x,4.38.0,x,x,00000000-0000-0000-0000-000000000391").unwrap();
        assert!(!clara.is_tested());
        // Every tested model is one Parallax can name.
        for id in crate::support::TESTED_MODELS {
            assert!(crate::screen::find(id).is_some(), "{id}");
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(VersionInfo::parse("").is_err());
        assert!(VersionInfo::parse("N1,x,not-a-version").is_err());
    }
}
