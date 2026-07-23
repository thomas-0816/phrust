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

/// Classifies the fixed registry families whose complete semantics live in
/// the VM. This runs once while the immutable builtin registry is built;
/// generated warm calls consume only the resulting numeric execution kind.
///
/// Function-pointer equality is deliberately not used here. Release codegen
/// may merge functions, so pointer identity cannot be a publication-time type
/// tag for the builtin execution plane.
pub(in crate::builtins) fn requires_vm_dispatch(name: &str) -> bool {
    matches!(
        name,
        "array_all"
            | "array_any"
            | "array_filter"
            | "array_find"
            | "array_find_key"
            | "array_map"
            | "array_multisort"
            | "array_reduce"
            | "array_walk"
            | "array_walk_recursive"
            | "arsort"
            | "asort"
            | "call_user_func"
            | "call_user_func_array"
            | "class_exists"
            | "class_implements"
            | "constant"
            | "debug_backtrace"
            | "debug_print_backtrace"
            | "defined"
            | "enum_exists"
            | "error_clear_last"
            | "error_get_last"
            | "error_log"
            | "error_reporting"
            | "exec"
            | "extension_loaded"
            | "flush"
            | "forward_static_call"
            | "func_get_arg"
            | "func_get_args"
            | "func_num_args"
            | "function_exists"
            | "get_called_class"
            | "get_cfg_var"
            | "get_class"
            | "get_class_methods"
            | "get_class_vars"
            | "get_current_user"
            | "get_declared_classes"
            | "get_declared_interfaces"
            | "get_declared_traits"
            | "get_loaded_extensions"
            | "get_mangled_object_vars"
            | "get_object_vars"
            | "get_parent_class"
            | "getenv"
            | "ignore_user_abort"
            | "ini_get"
            | "ini_get_all"
            | "ini_set"
            | "interface_exists"
            | "is_a"
            | "is_subclass_of"
            | "iterator_apply"
            | "iterator_count"
            | "iterator_to_array"
            | "krsort"
            | "ksort"
            | "method_exists"
            | "natcasesort"
            | "natsort"
            | "ob_end_clean"
            | "ob_end_flush"
            | "ob_get_clean"
            | "ob_get_contents"
            | "ob_get_flush"
            | "ob_get_length"
            | "ob_get_level"
            | "ob_start"
            | "passthru"
            | "pclose"
            | "php_sapi_name"
            | "php_uname"
            | "phpversion"
            | "popen"
            | "preg_replace_callback"
            | "preg_replace_callback_array"
            | "proc_close"
            | "proc_get_status"
            | "proc_open"
            | "property_exists"
            | "putenv"
            | "restore_error_handler"
            | "restore_exception_handler"
            | "rsort"
            | "set_error_handler"
            | "set_exception_handler"
            | "settype"
            | "shell_exec"
            | "sort"
            | "spl_autoload_call"
            | "spl_autoload_functions"
            | "spl_autoload_register"
            | "spl_autoload_unregister"
            | "system"
            | "trait_exists"
            | "trigger_error"
            | "uasort"
            | "uksort"
            | "user_error"
            | "usort"
            | "var_dump"
    )
}
