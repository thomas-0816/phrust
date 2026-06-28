--TEST--
opcache: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for Opcache/JIT classification without implementing a cache subsystem.
--FILE--
<?php
var_dump(extension_loaded("opcache"));
?>
--EXPECT--
bool(false)
