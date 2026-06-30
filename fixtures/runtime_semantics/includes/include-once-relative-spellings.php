<?php
// runtime-semantics: category=includes expect=pass
chdir(__DIR__ . "/_data/once");
$count = 0;
include_once "counter.php";
include_once "./counter.php";
echo $count, "\n";
