fn main() {
    // Die quietly on a closed pipe (`j3c doc css | head`) instead of panicking in `print!`.
    #[cfg(unix)]
    {
        extern "C" {
            fn signal(sig: i32, handler: usize) -> usize;
        }
        unsafe { signal(13, 0) };
    }
    let code = j3c::cli::run(std::env::args().skip(1));
    std::process::exit(code);
}
