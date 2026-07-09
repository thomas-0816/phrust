--TEST--
gmp symbols and constants
--SKIPIF--
<?php if (!extension_loaded("gmp")) die("skip gmp extension not loaded"); ?>
--FILE--
<?php
var_dump(GMP_ROUND_ZERO);
var_dump(GMP_ROUND_PLUSINF);
var_dump(GMP_ROUND_MINUSINF);
var_dump(GMP_MSW_FIRST);
var_dump(GMP_LSW_FIRST);
var_dump(GMP_LITTLE_ENDIAN);
var_dump(GMP_BIG_ENDIAN);
var_dump(GMP_NATIVE_ENDIAN);
var_dump(GMP_VERSION);
var_dump(function_exists("gmp_jacobi"));
var_dump(function_exists("gmp_legendre"));
var_dump(function_exists("gmp_kronecker"));
var_dump(function_exists("gmp_random_seed"));
var_dump(gmp_jacobi("1001", "9907"));
var_dump(gmp_legendre("1001", "9907"));
var_dump(gmp_kronecker("10", "21"));
var_dump(gmp_kronecker("10", "0"));
var_dump(gmp_kronecker("0", "10"));
var_dump(gmp_kronecker("-1", "3"));
var_dump(gmp_kronecker("-1", "4"));
var_dump(gmp_random_seed("123"));
?>
--EXPECT--
int(0)
int(1)
int(2)
int(1)
int(2)
int(4)
int(8)
int(16)
string(5) "6.3.0"
bool(true)
bool(true)
bool(true)
bool(true)
int(-1)
int(-1)
int(-1)
int(0)
int(0)
int(-1)
int(1)
NULL
