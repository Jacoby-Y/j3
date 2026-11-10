fn main() {
    // Die quietly on a closed pipe (`j3c doc css | head`) instead of panicking in `print!`.
    #[cfg(unix)]
    {
        extern "C" {
            fn signal(sig: i32, handler: usize) -> usize;
        }
        unsafe { signal(13, 0) };
    }
    j3c::cli::ice_hook();
    // Release builds abort on panic, so the hook exits; this catch is for unwinding (dev) builds.
    let code = std::panic::catch_unwind(|| j3c::cli::run(std::env::args().skip(1))).unwrap_or(j3c::cli::ICE_EXIT);
    std::process::exit(code);
}
