--TEST--
phrust-php CLI argv and argc superglobals
--ARGS--
one two
--FILE--
<?php
echo $argc, "\n";
echo $argv[1], "|", $_SERVER['argv'][2], "\n";
echo $_SERVER['argc'], "\n";
--EXPECT--
3
one|two
3
