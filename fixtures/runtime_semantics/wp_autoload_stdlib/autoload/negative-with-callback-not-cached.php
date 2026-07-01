<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
$calls = 0;
spl_autoload_register(function ($class) use (&$calls) {
    $calls++;
});

var_dump(class_exists("PackBNegativeCached"));
var_dump(class_exists("PackBNegativeCached"));
echo "calls=", $calls, "\n";
