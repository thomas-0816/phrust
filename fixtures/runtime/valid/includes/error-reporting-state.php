<?php
error_reporting(E_ALL);
include __DIR__ . "/lib/mask-deprecations.php";
echo (error_reporting() & E_DEPRECATED) === 0 ? "masked\n" : "visible\n";
$array = ['' => 7];
echo $array[null], "\n";
