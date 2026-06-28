<?php
// runtime-fixture: kind=valid diagnostic_id=E_PHP_VM_INCLUDE_MISSING
echo "before|";
include (__DIR__ . "/lib/missing.php");
echo "after\n";
