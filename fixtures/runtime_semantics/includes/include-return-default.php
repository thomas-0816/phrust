<?php
// runtime-semantics: category=includes expect=pass
$value = include (__DIR__ . "/_data/no-return.php");
echo $value, "\n";
