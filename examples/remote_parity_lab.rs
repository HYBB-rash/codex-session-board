#[path = "remote_parity_lab/engine.rs"]
pub mod engine;

fn main() {
    println!("Run cargo test --example remote_parity_lab --locked --offline");
}

#[cfg(test)]
#[path = "remote_parity_lab/host_isolation_tests.rs"]
mod host_isolation_tests;

#[cfg(test)]
#[path = "remote_parity_lab/surface.rs"]
mod surface;

#[cfg(test)]
#[path = "remote_parity_lab/remote_title_tests.rs"]
mod remote_title_tests;

#[cfg(test)]
#[path = "remote_parity_lab/remote_runtime_tests.rs"]
mod remote_runtime_tests;

#[cfg(test)]
#[path = "remote_parity_lab/remote_archive_tests.rs"]
mod remote_archive_tests;
