mod api;

fn main() {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    api::start();
}
