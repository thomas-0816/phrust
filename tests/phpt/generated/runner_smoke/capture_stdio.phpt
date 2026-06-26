--TEST--
PHPT runner CAPTURE_STDIO smoke
--CAPTURE_STDIO--
STDOUT STDERR
--FILE--
<?php
echo "stdout\n";
fwrite(STDERR, "stderr\n");
--EXPECT--
stdout
stderr
