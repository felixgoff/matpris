fn main() {
    // No commands for the page: only the app's Rust side (src-tauri/src/ica.rs) calls the
    // native `mint`, and that doesn't go through the page's permissions.
    tauri_plugin::Builder::new(&[]).android_path("android").ios_path("ios").build();
}
