--TEST--
sapi: CLI-only target policy stays explicit
--DESCRIPTION--
Generated Branch 4 data-platform coverage for SAPI policy without production web SAPI implementation.
--FILE--
<?php
echo php_sapi_name(), "\n";
?>
--EXPECT--
cli
