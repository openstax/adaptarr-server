extern crate adaptarr_server;

fn main() {
    if let Err(err) = adaptarr_server::main() {
        eprintln!("Error: {}", err);

        for cause in err.iter_causes() {
            eprintln!("Caused by: {}", cause);
        }
    }
}
