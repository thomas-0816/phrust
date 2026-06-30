<?php
// runtime-semantics: category=includes expect=fail
require (__DIR__ . "/_data/broken.php");
echo "unreachable\n";
