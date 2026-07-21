pub mod array_intrinsics;
pub(in crate::builtins) mod arrays;
pub(in crate::builtins) mod bcmath;
pub(in crate::builtins) mod calendar;
pub(in crate::builtins) mod core;
pub(in crate::builtins) mod curl;
pub(in crate::builtins) mod date;
pub(in crate::builtins) mod debug_output;
pub(in crate::builtins) mod exif;
pub(in crate::builtins) mod fileinfo;
pub(in crate::builtins) mod filesystem;
pub(in crate::builtins) mod filter;
pub(in crate::builtins) mod ftp;
pub(in crate::builtins) mod gd;
pub(in crate::builtins) mod gettext;
pub(in crate::builtins) mod gmp;
pub(in crate::builtins) mod hash;
pub(in crate::builtins) mod iconv;
pub(in crate::builtins) mod igbinary;
pub(in crate::builtins) mod imap;
pub(in crate::builtins) mod intl;
pub(in crate::builtins) mod json;
pub mod json_fast;
pub(in crate::builtins) mod ldap;
pub(in crate::builtins) mod math;
pub(in crate::builtins) mod mbstring;
pub(in crate::builtins) mod memcached;
pub(in crate::builtins) mod msgpack;
pub(in crate::builtins) mod mysqli;
pub(in crate::builtins) mod opcache;
pub(in crate::builtins) mod openssl;
pub(in crate::builtins) mod pcntl;
pub(in crate::builtins) mod pcre;
pub(in crate::builtins) mod pdo;
pub(in crate::builtins) mod pgsql;
pub(in crate::builtins) mod posix;
pub(in crate::builtins) mod readline;
pub(in crate::builtins) mod redis;
pub(in crate::builtins) mod reflection;
pub(in crate::builtins) mod session;
pub(in crate::builtins) mod shmop;
pub(in crate::builtins) mod simplexml;
pub(in crate::builtins) mod soap;
pub(in crate::builtins) mod sockets;
pub(in crate::builtins) mod sodium;
pub(in crate::builtins) mod spl;
pub(in crate::builtins) mod ssh2;
pub(in crate::builtins) mod streams;
pub mod string_intrinsics;
pub(in crate::builtins) mod strings;
pub(in crate::builtins) mod sysvmsg;
pub(in crate::builtins) mod sysvsem;
pub(in crate::builtins) mod sysvshm;
pub(in crate::builtins) mod xml;
pub(in crate::builtins) mod zip;
pub(in crate::builtins) mod zlib;

use super::InternalFunction;

/// Classifies the small set of registry stubs whose complete semantics live
/// in the VM. This runs once while the immutable builtin registry is built;
/// generated warm calls consume only the resulting numeric execution kind.
pub(in crate::builtins) fn requires_vm_dispatch(name: &str, function: InternalFunction) -> bool {
    let function = function as usize;
    let stub = function == arrays::builtin_array_callback_requires_vm as InternalFunction as usize
        || function == arrays::builtin_array_sort_requires_vm as InternalFunction as usize
        || function == core::builtin_config_requires_vm as InternalFunction as usize
        || function == core::builtin_environment_requires_vm as InternalFunction as usize
        || function == core::builtin_error_handling_requires_vm as InternalFunction as usize
        || function == core::builtin_output_buffering_requires_vm as InternalFunction as usize
        || function == core::builtin_process_requires_vm as InternalFunction as usize
        || function == core::builtin_settype_requires_vm as InternalFunction as usize
        || function == spl::builtin_spl_autoload_requires_vm as InternalFunction as usize
        || function
            == reflection::builtin_symbol_introspection_requires_vm as InternalFunction as usize;
    stub || matches!(
        name,
        "preg_replace_callback" | "preg_replace_callback_array" | "var_dump"
    )
}
