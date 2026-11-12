fn main() {
    // The UI is embedded by `generate_context!`, which Cargo cannot track on
    // its own: without this, editing only HTML/CSS/JS leaves a stale bundle.
    println!("cargo:rerun-if-changed=../ui");
    tauri_build::build()
}
