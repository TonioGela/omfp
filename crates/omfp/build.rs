//! Tauri's build step (context, ACL manifests) for the `desktop` feature only (M7).
fn main() {
    #[cfg(feature = "desktop")]
    tauri_build::build();
}
