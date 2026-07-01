<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
$first = function ($class) {
    echo "first:", $class, "\n";
};
$second = function ($class) {
    echo "second:", $class, "\n";
    if ($class === "PackBExists") {
        include __DIR__ . "/_data/PackBExists.php";
    }
};

spl_autoload_register($first);
spl_autoload_register($second);
var_dump(count(spl_autoload_functions()));
spl_autoload_unregister($first);
var_dump(count(spl_autoload_functions()));
var_dump(class_exists("PackBExists"));
