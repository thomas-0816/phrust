--TEST--
PHPT runner ARGS smoke
--ARGS--
alpha beta
--FILE--
<?php
echo $argc, "\n";
echo implode(",", $argv), "\n";
--EXPECTF--
3
%sargs.php,alpha,beta
