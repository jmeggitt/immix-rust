fn main() {
    #[cfg(target_os = "linux")]
    gcc::compile_library("libgc_clib_x64.a", &["src/heap/gc/clib_x64.c"]);

    // if cfg!(target_os = "linux") {
    //     cc::Config::new()
    //                  .flag("-lpfm")
    //                  .flag("-O3")
    //                  .file("src/common/perf.c")
    //                  .compile("libgc_perf.a");
    // }
}
