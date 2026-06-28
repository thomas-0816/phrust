--TEST--
phrust-php CLI INI overrides reach runtime and builtins
--INI--
include_path=.:cli-fixtures
display_errors=0
error_reporting=0
--FILE--
<?php
echo ini_get("include_path"), "\n";
echo ini_get("display_errors"), "\n";
echo error_reporting(), "\n";
--EXPECT--
.:cli-fixtures
0
0
