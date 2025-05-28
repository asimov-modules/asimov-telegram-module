// Constants for configuration
const SUPPORTED_PLATFORMS: &[(&str, &str)] = &[
    ("linux", "x86_64"),
    ("macos", "aarch64"),
    ("macos", "x86_64"),
];
const REQUIRED_LIBS: &[&str] = &[
    "tdactor",
    "tdapi",
    "tdclient",
    "tdcore",
    "tddb",
    "tdjson_private",
    "tdjson_static",
    "tdnet",
    "tdsqlite",
    "tdutils",
];
const LINK_LIBS: &[&str] = &["ssl", "crypto", "z"];
// const LINK_LIBS: &[&str] = &[];

fn main() {
    // Link the C++ runtime
    println!("cargo:rustc-link-lib=c++");

    for lib in LINK_LIBS {
        println!("cargo:rustc-link-lib=static={}", lib);
    }

    // Retrieve and validate target OS and architecture
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Failed to get target OS");
    let target_arch =
        std::env::var("CARGO_CFG_TARGET_ARCH").expect("Failed to get target architecture");

    // Check if the platform is supported
    if !SUPPORTED_PLATFORMS.contains(&(target_os.as_str(), target_arch.as_str())) {
        panic!("Unsupported platform: {}-{}", target_os, target_arch);
    }

    // Link TDLib based on the platform
    let tdlib_path = std::env::var("PRE_COMPILED_TDLIB")
        .expect("Environment variable PRE_COMPILED_TDLIB not found");

    link_tdlib(&tdlib_path);
}

fn link_tdlib(tdlib_path: &str) {
    link_library_path(tdlib_path);
    link_required_libraries();
}

fn link_library_path(tdlib_path: &str) {
    let lib_path = format!("{}/lib", tdlib_path);
    let include_path = format!("{}/include", tdlib_path);

    println!("cargo:rustc-link-search=native={}", lib_path);
    println!("cargo:include={}", include_path);
}

fn link_required_libraries() {
    for lib in REQUIRED_LIBS {
        println!("cargo:rustc-link-lib=static={}", lib);
    }
}
