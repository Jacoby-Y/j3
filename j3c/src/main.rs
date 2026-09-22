fn main() {
    let code = j3c::cli::run(std::env::args().skip(1));
    std::process::exit(code);
}
