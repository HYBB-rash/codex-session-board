pub mod app;
pub mod archive;
pub mod auto_archive;
pub mod config;
pub mod http;
pub mod mcp;
pub mod model;
pub mod native_sections;
pub mod observer;
pub mod preflight;
pub mod projects;
pub mod remote;
pub mod rpc;
pub mod service;
pub mod store;
pub mod titles;
mod workspaces;

#[cfg(test)]
mod formal;

mod workers;
