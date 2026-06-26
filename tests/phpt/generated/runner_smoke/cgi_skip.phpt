--TEST--
PHPT runner CGI policy smoke
--CGI--
--FILE--
<?php
echo "should not run\n";
--EXPECT--
should not run
