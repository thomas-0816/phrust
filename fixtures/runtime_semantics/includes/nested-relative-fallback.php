<?php
// runtime-semantics: category=includes expect=pass
chdir(__DIR__ . "/_data/cwd-empty");
ini_set("include_path", "");
include (__DIR__ . "/_data/nested/parent.php");
echo "\n";
