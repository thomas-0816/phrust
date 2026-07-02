<?php
// runtime-semantics: category=includes expect=pass
echo "before|";
$value = include (__DIR__ . "/_data/missing.php");
echo "|", $value === false ? "false" : "unexpected", "|after\n";
