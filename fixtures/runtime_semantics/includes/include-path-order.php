<?php
// runtime-semantics: category=includes expect=pass
ini_set("include_path", __DIR__ . "/_data/include_path/first:" . __DIR__ . "/_data/include_path/second");
include "chosen.php";
echo "\n";
