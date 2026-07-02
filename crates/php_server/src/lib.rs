//! Integrated HTTP server.
//!
//! `php_server` owns HTTP transport concerns: routing, static files, request
//! body limits, concurrency limits, response mapping, server metrics, and the
//! server CLI. PHP execution goes through `php_executor` in-process; the server
//! does not call FPM, FastCGI, CGI, Apache module hooks, `mod_php`, external
//! `php`, or `php-vm` subprocesses.

mod access_log;
pub mod config;
mod diagnostics;
mod metrics;
mod multipart;
mod php_request;
pub mod response;
pub mod routing;
mod serve;
pub mod server;
pub mod session_store;
mod sessions;
mod state;
mod static_files;
mod tls;
