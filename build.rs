fn main() {
    // Battery status and GPU-name lookups on macOS go through IOKit/CoreFoundation.
    // No-op on every other target.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=IOKit");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
}
