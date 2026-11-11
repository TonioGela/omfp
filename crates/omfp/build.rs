//! Tauri's build step (context, ACL manifests) for the `desktop` feature only (M7), the daemon's tray pixmaps
//! (UI-219): `icons/tray-<state>-<size>.png` (rendered from `icons/tray.html`) as the big-endian ARGB32 rows of a
//! `StatusNotifierItem` `IconPixmap`, in `$OUT_DIR/tray-<state>-<size>.argb`, and on macOS the embedded Info.plist.
fn main() {
    #[cfg(feature = "desktop")]
    {
        tauri_build::build();
        tray::pixmaps();
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
            info_plist();
        }
    }
}

/// macOS (UI-251, ENV-041): a plain executable has no bundle, so macOS reads the Info.plist embedded in
/// `__TEXT,__info_plist`: the bundle id of the notifications (UI-252) and `NSMicrophoneUsageDescription`, without
/// which `WKWebView` hides `navigator.mediaDevices`. Tauri embeds one only in dev builds (`custom-protocol` off).
#[cfg(feature = "desktop")]
fn info_plist() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    println!("cargo::rerun-if-changed=Info.plist");
    let template = std::fs::read_to_string("Info.plist").expect("cannot read Info.plist");
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let plist = out.join("Info.plist");
    std::fs::write(&plist, template.replace("@VERSION@", &version))
        .expect("cannot write Info.plist");
    println!(
        "cargo::rustc-link-arg-bin=omfp=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );
}

#[cfg(feature = "desktop")]
mod tray {
    use std::fs::File;
    use std::io::BufReader;
    use std::path::Path;

    /// The tray icon states and pixel sizes; `src/tray.rs` includes every combination.
    const STATES: [&str; 3] = ["idle", "running", "attention"];
    const SIZES: [u32; 2] = [32, 64];

    pub fn pixmaps() {
        let out = std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR");
        for state in STATES {
            for size in SIZES {
                let name = format!("tray-{state}-{size}");
                let source = Path::new("icons").join(format!("{name}.png"));
                println!("cargo::rerun-if-changed={}", source.display());
                let argb = argb(&source, size);
                std::fs::write(Path::new(&out).join(format!("{name}.argb")), argb)
                    .expect("cannot write the tray pixmap");
            }
        }
    }

    /// The RGBA PNG at `path` (`size`×`size`, 8 bits per channel) as ARGB32 in network byte order.
    fn argb(path: &Path, size: u32) -> Vec<u8> {
        let file = File::open(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let mut reader = png::Decoder::new(BufReader::new(file))
            .read_info()
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let mut rgba = vec![0; reader.output_buffer_size().expect("a small image")];
        let frame = reader
            .next_frame(&mut rgba)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            frame.color_type == png::ColorType::Rgba
                && frame.bit_depth == png::BitDepth::Eight
                && frame.width == size
                && frame.height == size,
            "{}: expected an 8-bit RGBA {size}x{size} image",
            path.display()
        );
        rgba.truncate(frame.buffer_size());
        let (pixels, _) = rgba.as_chunks::<4>();
        pixels
            .iter()
            .flat_map(|&[red, green, blue, alpha]| [alpha, red, green, blue])
            .collect()
    }
}
