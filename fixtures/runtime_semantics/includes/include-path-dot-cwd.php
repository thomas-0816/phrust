<?php
// runtime-semantics: category=includes expect=pass
chdir(__DIR__ . "/_data/cwd");
ini_set("include_path", ".");
include "dot-target.php";
echo "\n";
