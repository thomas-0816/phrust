<?php
// runtime-semantics: category=includes expect=known_gap known_gap=E_PHP_RUNTIME_WARNING_CHANNEL_COMPAT
echo "before|";
$value = include (__DIR__ . "/_data/missing.php");
echo "|", $value === false ? "false" : "unexpected", "|after\n";
