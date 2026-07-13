fn main() {
    #[cfg(not(feature = "full-runtime"))]
    return;

    #[cfg(feature = "full-runtime")]
    configure_libmagic();
}

#[cfg(feature = "full-runtime")]
fn configure_libmagic() {
    if let Ok(include_dir) = std::env::var("PHPRUST_LIBMAGIC_INCLUDE_DIR") {
        println!("cargo:include={include_dir}");
    }
    if let Ok(lib_dir) = std::env::var("PHPRUST_LIBMAGIC_LIB_DIR") {
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=magic");
        return;
    }
    if pkg_config::Config::new().probe("libmagic").is_err() {
        println!("cargo:rustc-link-lib=magic");
    }
}
