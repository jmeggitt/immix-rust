use cc::Build;

fn main() {
    Build::new()
        .file("src/heap/gc/clib_x64.c")
        .compile("gc_clib_x64");

    println!("cargo:rerun-if-changed=src/heap/gc/clib_x64.c");

    // if cfg!(target_os = "linux") {
    //     cc::Config::new()
    //                  .flag("-lpfm")
    //                  .flag("-O3")
    //                  .file("src/common/perf.c")
    //                  .compile("libgc_perf.a");
    // }
}
