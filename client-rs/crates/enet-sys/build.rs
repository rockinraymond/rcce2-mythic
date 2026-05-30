//! Compile the vendored RCEnet ENet fork (vendor/*.c) into a static lib.
//! win32.c is `#ifdef WIN32`, unix.c is `#ifndef WIN32`, so both are listed and
//! the inactive one compiles to nothing.

fn main() {
    let windows = std::env::var("CARGO_CFG_WINDOWS").is_ok();

    let mut b = cc::Build::new();
    b.include("vendor/include");
    b.warnings(false);
    for f in [
        "callbacks.c",
        "host.c",
        "list.c",
        "packet.c",
        "peer.c",
        "protocol.c",
        "unix.c",
        "win32.c",
    ] {
        b.file(format!("vendor/{f}"));
    }
    if windows {
        // ENet's platform code keys off WIN32 (no underscore); MSVC only
        // guarantees _WIN32, so define WIN32 explicitly.
        b.define("WIN32", None);
    }
    b.compile("rcenet_c");

    if windows {
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=winmm");
    }

    println!("cargo:rerun-if-changed=vendor");
}
