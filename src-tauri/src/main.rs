// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // A widespread WebKitGTK bug on certain Linux GPU driver combinations
    // renders the window frame fine but leaves the webview content
    // permanently blank. These env vars must be set before WebKitGTK
    // initializes (i.e. before `run()` creates the window), and are
    // meaningless on Windows/macOS (different webview engines), hence the
    // cfg gate rather than a runtime check.
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    mint_launcher_lib::run()
}
