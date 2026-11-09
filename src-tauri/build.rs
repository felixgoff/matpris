fn main() {
    // The updater's public key comes from the release build's environment
    // (TAURI_UPDATER_PUBKEY); without it the app doesn't update itself.
    println!("cargo:rerun-if-env-changed=TAURI_UPDATER_PUBKEY");
    tauri_build::build()
}
