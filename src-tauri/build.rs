fn main() {
    link_compiler_rt();
    tauri_build::build()
}

/// ggml-metal usa `@available`, que necesita `___isPlatformVersionAtLeast` de compiler-rt.
/// Rust enlaza con `-nodefaultlibs`, así que la añadimos a mano.
fn link_compiler_rt() {
    let Ok(out) = std::process::Command::new("xcrun").args(["clang", "--print-resource-dir"]).output() else { return };
    let dir = std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()).join("lib/darwin");
    if dir.join("libclang_rt.osx.a").is_file() {
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }
}
