fn main() {
    let mut attrs = tauri_build::Attributes::new();
    attrs = attrs.app_manifest(tauri_build::AppManifest::new().commands(&[
        "device_restore", "reader_list", "reader_entry", "reader_file", "reader_config",
        "reader_books", "reader_close", "spike_input", "spike_snapshot",
        "reader_save_position", "report",
    ]));
    tauri_build::try_build(attrs).expect("tauri build");
}
