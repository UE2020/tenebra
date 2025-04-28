use std::env;
use std::path::PathBuf;

fn main() {
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Generate bindings for the C header
        let bindings = bindgen::Builder::default()
            .header("input-binding/multitouch_simulator.h") // Path to your C header
            .generate()
            .expect("Unable to generate bindings");

        // Write the bindings to the appropriate location in OUT_DIR
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");

        cc::Build::new()
            .file("input-binding/multitouch_simulator.c")
            .compile("multitouch_simulator");

        println!("cargo:rerun-if-changed=input-binding/multitouch_simulator.c");
        println!("cargo:rerun-if-changed=input-binding/multitouch_simulator.h");
    }
}
