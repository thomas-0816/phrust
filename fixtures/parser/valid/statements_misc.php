<?php

global $a, $b;
static $count = 0, $name;

unset($a, $b);

if (isset($a, $b) && empty($name)) {
    print "empty";
}

$value = include __DIR__ . "/file.php";
$again = include_once __FILE__;
$config = require __DIR__ . "/config.php";
require_once __FILE__;

$result = eval("return 1;");

exit;
die("done");

start_label:
goto start_label;
