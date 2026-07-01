<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    echo "autoload:", $class, "\n";
    include __DIR__ . "/_data/PackBNoAutoload.php";
});

var_dump(class_exists("PackBNoAutoload", false));
var_dump(class_exists("PackBNoAutoload", true));
