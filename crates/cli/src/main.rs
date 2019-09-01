fn main() {
    if let Err(err) = adaptarr_cli::main() {
        eprintln!("Error: {}", err);

        for cause in err.iter_causes() {
            eprintln!("Caused by: {}", cause);
        }

        eprintln!("{}", err.backtrace());
    }
}
