//! Integrated HTTP server.
//!
//! `php_server` owns HTTP transport concerns: routing, static files, request
//! body limits, concurrency limits, response mapping, server metrics, and the
//! server CLI. PHP execution goes through `php_executor` in-process; the server
//! does not call FPM, FastCGI, CGI, Apache module hooks, `mod_php`, external
//! `php`, or `php-vm` subprocesses.

pub mod config;
mod multipart;
pub mod response;
pub mod routing;
pub mod server;
