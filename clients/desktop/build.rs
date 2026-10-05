use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use image::ImageEncoder;

#[path = "src/taskbar_icon.rs"]
mod taskbar_icon;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("apple-darwin") {
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=LocalAuthentication");
    }
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }
    // The embedded icon is taskbar_icon::taskbar_rgba. Rebuild when that
    // drawing changes so the exe resource cannot keep an older radius.
    println!("cargo:rerun-if-changed=src/taskbar_icon.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ico = out_dir.join("Drop.ico");
    write_icon(&ico);

    // OUT_DIR is target/<triple>/<profile>/build/drop-desktop-<hash>/out.
    // The installer reads Drop.ico from the profile directory, next to the exe.
    let mut beside_exe = out_dir.clone();
    beside_exe.pop();
    beside_exe.pop();
    beside_exe.pop();
    beside_exe.push("Drop.ico");
    fs::copy(&ico, &beside_exe).unwrap_or_else(|error| {
        panic!("copy {} to {}: {error}", ico.display(), beside_exe.display())
    });

    let ico_rc = ico.display().to_string().replace('\\', "/");
    let id = taskbar_icon::ICON_RESOURCE_ID;
    let rc_path = out_dir.join("drop.rc");
    fs::write(
        &rc_path,
        format!(
            r#"{id} ICON "{ico_rc}"

1 VERSIONINFO
FILEVERSION 1,0,0,0
PRODUCTVERSION 1,0,0,0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Drop"
      VALUE "FileVersion", "1.0.0"
      VALUE "InternalName", "Drop"
      VALUE "OriginalFilename", "Drop.exe"
      VALUE "ProductName", "Drop"
      VALUE "ProductVersion", "1.0.0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
        ),
    )
    .expect("write drop.rc");

    let windres = env::var("WINDRES").unwrap_or_else(|_| {
        if target.contains("windows-gnu") {
            "x86_64-w64-mingw32-windres".into()
        } else {
            "windres".into()
        }
    });
    let res = out_dir.join("drop.res");
    let status = Command::new(&windres)
        .args(["--target=pe-x86-64", "-O", "coff"])
        .arg(&rc_path)
        .arg(&res)
        .status()
        .unwrap_or_else(|error| panic!("{windres} is required to embed the Windows icon: {error}"));
    if !status.success() {
        panic!("{windres} failed");
    }
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

fn write_icon(path: &std::path::Path) {
    const SIZES: [u32; 5] = [16, 32, 64, 128, 256];
    let mut images = Vec::with_capacity(SIZES.len());
    for size in SIZES {
        let rgba = taskbar_icon::taskbar_rgba(size);
        // At 16 the corner pixel stays covered. At 32 it is only partly
        // covered. From 64 up the corner sample is outside the arc.
        if size >= 64 {
            assert_eq!(rgba[3], 0, "taskbar icon corner must be transparent at {size}");
        }
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&rgba, size, size, image::ExtendedColorType::Rgba8)
            .unwrap_or_else(|error| panic!("encode {size}px taskbar icon: {error}"));
        images.push((size, png));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create icon directory");
    }
    let mut file = fs::File::create(path).expect("create Drop.ico");
    file.write_all(&pack_ico(&images)).expect("write Drop.ico");
}

/// Vista-style icon: PNG images inside an ICO directory.
fn pack_ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = images.len() as u16;
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    let mut payload = Vec::new();
    for (width, data) in images {
        let stored = if *width >= 256 { 0 } else { *width as u8 };
        out.push(stored);
        out.push(stored);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
        payload.extend_from_slice(data);
    }
    out.extend_from_slice(&payload);
    out
}
