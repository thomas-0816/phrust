<?php
// runtime-fixture: kind=valid
define('PHRUST_BEFORE_TOP_LEVEL_ISSET_INCLUDE', true);
require __DIR__ . '/lib/top-level-isset-uninitialized.php';
echo "|done\n";
