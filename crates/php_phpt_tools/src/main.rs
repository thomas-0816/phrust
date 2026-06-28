use std::env;
use std::io;

mod cli;
mod commands;

fn main() {
    let code = match cli::run(env::args().skip(1), &mut io::stdout(), &mut io::stderr()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            2
        }
    };
    if code != 0 {
        std::process::exit(code);
    }
}
