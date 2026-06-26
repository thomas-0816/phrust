--TEST--
Runner skips PHPTs that require unavailable extensions
--EXTENSIONS--
phrust_runner_missing_extension
--FILE--
<?php
echo "should not run\n";
?>
--EXPECT--
should not run
