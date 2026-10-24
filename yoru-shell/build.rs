//! The shell's markup (`ui/*.tole`), compiled into Rust by tole.

fn main() {
    if tole_compiler::build("ui").is_err() {
        std::process::exit(1);
    }
}
