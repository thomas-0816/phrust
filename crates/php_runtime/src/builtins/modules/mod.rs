pub(in crate::builtins) mod apcu;
pub mod array_intrinsics;
pub(in crate::builtins) mod arrays;
pub(in crate::builtins) mod bcmath;
pub(in crate::builtins) mod calendar;
pub(in crate::builtins) mod core;
pub(in crate::builtins) mod ctype;
#[cfg(not(target_family = "wasm"))]
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
#[cfg(not(target_family = "wasm"))]
pub(in crate::builtins) mod mysqli;
pub(in crate::builtins) mod opcache;
#[cfg(not(target_family = "wasm"))]
pub(in crate::builtins) mod openssl;

#[cfg(not(target_family = "wasm"))]
pub(in crate::builtins) mod pcntl;

pub(in crate::builtins) mod pcre;
pub(in crate::builtins) mod pdo;
#[cfg(not(target_family = "wasm"))]
pub(in crate::builtins) mod pgsql;

#[cfg(not(target_family = "wasm"))]
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
