--TEST--
PHPT runner PHPDBG policy smoke
--PHPDBG--
run
--FILE--
<?php
echo "should not run\n";
--EXPECT--
should not run
