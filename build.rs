use std::{env, process::Command};

fn main() {
    let short_sha = output(Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"]));
    let date = output(Command::new("git")
        .args(&["log", "-1", "--date=short", "--pretty=%cd"]));

    let version = env::var("CARGO_PKG_VERSION").unwrap();

    let version = format!("{} ({} {})", version, short_sha, date);
    println!("cargo:rustc-env=VERSION={}", version);
}

fn output(cmd: &mut Command) -> String {
    let out = cmd.output().expect("could not run command");

    if !out.status.success() {
        panic!("could not run command: {}", String::from_utf8_lossy(&out.stderr));
    }

    String::from_utf8(out.stdout)
        .expect("command returned invalid Unicode")
        .trim()
        .to_string()
}
