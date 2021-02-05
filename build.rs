fn main() {
    println!("cargo:rerun-if-changed=src/shim.c");
    let mut build = cc::Build::new();
    build.file("src/shim.c");
    build.compile("tidshim");
    println!("cargo:rustc-link-lib=static=tidshim");
}
