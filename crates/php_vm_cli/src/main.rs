//! VM CLI process entry point.
//!
//! Command parsing and debug/report adapters live in `commands`; reusable
//! library entrypoints live in `php_vm_cli`.

mod commands;
mod todo_cli;

fn main() {
    commands::main_entry();
}
