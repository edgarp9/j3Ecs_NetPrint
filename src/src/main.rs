#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    if let Err(error) = j3ecs_netprint::app::run() {
        eprintln!("j3ecs-netprint failed: {error}");
        std::process::exit(1);
    }
}
