#[path = "remote_parity_lab/engine.rs"]
pub mod engine;

fn main() {
    println!("Run cargo test --example remote_parity_lab --locked --offline");
}

#[cfg(test)]
#[path = "remote_parity_lab/host_isolation_tests.rs"]
mod host_isolation_tests;
