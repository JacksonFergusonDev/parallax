//! A reading layout (font, size, spacing, margins, alignment) kept in the
//! library's settings and set on books on a Kobo, as Kobo Utilities' "Set
//! reader font settings" does.

use std::path::Path;

use parallax_device_kobo::{Alignment, KoboDevice, Layout, apply_layout};
use parallax_library::Library;

/// `on` to give library books on a Kobo the layout when they have none of
/// their own, each time a Kobo connects.
pub const KOBO_LAYOUT: &str = "kobo_layout";
pub const KOBO_LAYOUT_FONT: &str = "kobo_layout_font";
pub const KOBO_LAYOUT_SIZE: &str = "kobo_layout_size";
pub const KOBO_LAYOUT_LINE_HEIGHT: &str = "kobo_layout_line_height";
pub const KOBO_LAYOUT_MARGINS: &str = "kobo_layout_margins";
/// `left` or `justify`; unset is as the book sets it.
pub const KOBO_LAYOUT_ALIGNMENT: &str = "kobo_layout_alignment";
/// Every setting the layout is made of.
pub const KOBO_LAYOUT_SETTINGS: [&str; 6] = [
    KOBO_LAYOUT,
    KOBO_LAYOUT_FONT,
    KOBO_LAYOUT_SIZE,
    KOBO_LAYOUT_LINE_HEIGHT,
    KOBO_LAYOUT_MARGINS,
    KOBO_LAYOUT_ALIGNMENT,
];

/// The layout the settings describe. Unreadable or unset parts are left to
/// Nickel.
pub fn saved_layout(library: &Library) -> Layout {
    let get = |key| library.setting(key).ok().flatten();
    Layout {
        font_family: get(KOBO_LAYOUT_FONT),
        font_size: get(KOBO_LAYOUT_SIZE).and_then(|v| v.parse().ok()),
        line_height: get(KOBO_LAYOUT_LINE_HEIGHT).and_then(|v| v.parse().ok()),
        margins: get(KOBO_LAYOUT_MARGINS).and_then(|v| v.parse().ok()),
        alignment: match get(KOBO_LAYOUT_ALIGNMENT).as_deref() {
            Some("left") => Some(Alignment::Left),
            Some("justify") => Some(Alignment::Justify),
            _ => None,
        },
    }
}

/// Sets `layout` on these books. Returns how many changed.
pub fn apply(
    device: &KoboDevice,
    backup_root: &Path,
    content_ids: &[String],
    layout: &Layout,
) -> parallax_device_kobo::Result<usize> {
    if content_ids.is_empty() || layout.is_empty() {
        return Ok(0);
    }
    let mut writer = device.open_writer(backup_root)?;
    let changed = apply_layout(&mut writer, content_ids, layout);
    writer.close()?;
    changed
}

/// Library books on `device` that Nickel has imported and that have no
/// reading settings of their own.
pub fn pending(
    library: &Library,
    device: &KoboDevice,
) -> parallax_device_kobo::Result<Vec<String>> {
    let serial = device.version().serial.clone();
    let copies = library.device_copies(&serial).unwrap_or_default();
    if copies.is_empty() {
        return Ok(Vec::new());
    }
    let db = device.open_db()?;
    let set = db.books_with_layout()?;
    let imported: Vec<String> = db.books()?.into_iter().map(|b| b.id).collect();
    Ok(copies
        .into_iter()
        .map(|c| c.content_id)
        .filter(|id| imported.contains(id) && !set.contains(id))
        .collect())
}

/// Gives library books without their own settings the saved layout.
pub fn apply_to_new(
    library: &Library,
    device: &KoboDevice,
    backup_root: &Path,
) -> parallax_device_kobo::Result<usize> {
    let layout = saved_layout(library);
    if layout.is_empty() {
        return Ok(0);
    }
    apply(device, backup_root, &pending(library, device)?, &layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_layout_from_settings() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();
        assert!(saved_layout(&library).is_empty());
        for (key, value) in [
            (KOBO_LAYOUT_FONT, "Georgia"),
            (KOBO_LAYOUT_SIZE, "22"),
            (KOBO_LAYOUT_LINE_HEIGHT, "1.35"),
            (KOBO_LAYOUT_MARGINS, "3"),
            (KOBO_LAYOUT_ALIGNMENT, "justify"),
        ] {
            library.set_setting(key, Some(value)).unwrap();
        }
        assert_eq!(
            saved_layout(&library),
            Layout {
                font_family: Some("Georgia".into()),
                font_size: Some(22),
                line_height: Some(1.35),
                margins: Some(3),
                alignment: Some(Alignment::Justify),
            }
        );
    }
}
