<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    echo "autoload:", $class, "\n";
    if ($class === "PackBExists") {
        include __DIR__ . "/_data/PackBExists.php";
    }
});

var_dump(class_exists("PackBExists"));
