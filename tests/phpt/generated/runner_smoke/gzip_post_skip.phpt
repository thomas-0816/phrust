--TEST--
PHPT runner GZIP_POST policy smoke
--GZIP_POST--
a=1
--FILE--
<?php
echo "should not run\n";
--EXPECT--
should not run
