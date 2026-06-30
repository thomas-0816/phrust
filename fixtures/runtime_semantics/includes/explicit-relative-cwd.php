<?php
// runtime-semantics: category=includes expect=pass
chdir(__DIR__ . "/_data/cwd");
ini_set("include_path", __DIR__ . "/_data/include_path/first");
include "./explicit.php";
echo "\n";
