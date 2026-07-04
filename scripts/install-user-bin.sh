#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo build -p php_vm_cli -p php_server --bins

bin_dir="$repo_root/target/phrust/bin"
mkdir -p "$bin_dir"

ln -sf "$repo_root/target/debug/phrust-php" "$bin_dir/phrust-php"
ln -sf "$repo_root/target/debug/phrust-server" "$bin_dir/phrust-server"
ln -sf "$repo_root/target/debug/php-vm" "$bin_dir/php-vm"
ln -sf "$repo_root/target/debug/phrust-php" "$bin_dir/php"

printf 'Installed phrust user binaries in %s\n' "$bin_dir"
printf 'Run: export PATH="%s:$PATH"\n' "$bin_dir"
