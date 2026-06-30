<?php
// runtime-semantics: category=includes expect=fail
echo "before|";
require (__DIR__ . "/_data/missing.php");
echo "after\n";
