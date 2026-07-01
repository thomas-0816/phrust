<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
$calls = 0;
spl_autoload_register(function ($class) use (&$calls) {
    $calls++;
    class_exists($class);
});

var_dump(class_exists("PackBMissingRecursive"));
echo "calls=", $calls, "\n";
