--TEST--
PHPT runner DEFLATE_POST policy smoke
--DEFLATE_POST--
a=1
--FILE--
<?php
echo "should not run\n";
--EXPECT--
should not run
