--TEST--
mysqlnd: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for mysqlnd classification as a non-userland native MySQL driver surface.
--FILE--
<?php
var_dump(extension_loaded("mysqlnd"));
?>
--EXPECT--
bool(false)
