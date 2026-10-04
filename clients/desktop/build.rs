use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
    println!("cargo:rerun-if-changed=make-windows-icon.py");
    println!("cargo:rerun-if-changed=make-icon.py");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ico = out_dir.join("Drop.ico");
    let python = Command::new("python3")
        .arg(manifest.join("make-windows-icon.py"))
        .arg(&ico)
        .status()
        .unwrap_or_else(|error| panic!("python3 is required to build the Windows icon: {error}"));
    if !python.success() {
        panic!("make-windows-icon.py failed");
    }

    let ico_rc = ico.display().to_string().replace('\\', "/");
    let rc_path = out_dir.join("drop.rc");
    fs::write(
        &rc_path,
        format!(
            r#"1 ICON "{ico_rc}"

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
