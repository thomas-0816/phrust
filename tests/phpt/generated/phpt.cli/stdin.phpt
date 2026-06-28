--TEST--
phrust-php CLI STDIN is exposed to PHP code
--STDIN--
payload from stdin
--FILE--
<?php
echo stream_get_contents(STDIN), "\n";
--EXPECT--
payload from stdin
