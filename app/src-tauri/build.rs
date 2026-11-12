//! The app's commands are listed here so Tauri enforces its ACL on them.
//! Each one also needs granting in `capabilities/`; a window with no grant
//! can't call it. `lib.rs` tests that this list matches `generate_handler!`
//! and that each command is granted only to the windows that need it.

const COMMANDS: &[&str] = &[
    "show_window",
    "about_info",
    "about_open_repository",
    "list_devices",
    "library_list",
    "library_import",
    "library_review",
    "library_update",
    "library_set_comic_kind",
    "library_comic_spreads",
    "library_remove",
    "goodreads_plan",
    "goodreads_import",
    "goodreads_saved",
    "goodreads_edit",
    "library_snapshots",
    "library_restore",
    "device_backups",
    "device_restore",
    "books_on_device",
    "device_books",
    "remove_from_device",
    "update_books",
    "tidy_device",
    "device_history",
    "send_books",
    "comic_transfer_estimate",
    "import_from_device",
    "calibre_default_library",
    "reading_stats",
    "genres_plan",
    "genres_apply",
    "calibre_preview",
    "calibre_import",
    "library_duplicates",
    "enhance_propose",
    "open_metadata_source",
    "kobo_health",
    "library_check",
    "annotations_list",
    "annotations_export",
    "device_report",
    "clean_images",
    "merge_collections",
    "set_reading_status",
    "enhance_apply",
    "setting_get",
    "setting_set",
    "settings_all",
    "settings_reset",
    "backup_settings",
    "backup_settings_set",
    "backup_delete_all",
    "show_folder",
    "reader_settings_get",
    "reader_settings_set",
    "menu_state",
    "update_available",
    "update_install",
    "update_skip",
    "open_help",
    "device_duplicates",
    "dismiss_duplicates",
    "list_collections",
    "change_collections",
    "folder_groups",
    "reconcile_device",
    "rules_list",
    "rules_save",
    "rules_delete",
    "rules_preview",
    "rules_apply",
    "eject_device",
    "cover_search",
    "cover_set_url",
    "cover_set_file",
    "cover_reset",
    "cover_info",
    "cover_upscale_available",
    "cover_upscale_tiles",
    "cover_upscale_preview",
    "cover_upscale_apply",
    "cover_upscale_cancel",
    "cover_colors",
    "reader_open",
    "reader_book",
    "reader_entry",
    "reader_save_position",
    "reader_save_settings",
    "reader_highlights",
    "reader_save_highlight",
    "reader_delete_highlight",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));

    let attributes = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Tauri embeds the manifest only in the app executable. The test
        // harness also needs Common Controls v6 to load (tauri#13419).
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        attributes
    };

    tauri_build::try_build(attributes).expect("failed to run the Tauri build script");
}
