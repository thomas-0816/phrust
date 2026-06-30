<?php
// runtime-semantics: category=includes expect=pass
chdir(__DIR__ . "/_data/once");
$count = 0;
$first = include_once "counter.php";
$second = require_once __DIR__ . "/_data/once/./counter.php";
$third = include_once __DIR__ . "/_data/once/counter.php";
echo $count, ":", $first, ":", $second, ":", $third, "\n";
