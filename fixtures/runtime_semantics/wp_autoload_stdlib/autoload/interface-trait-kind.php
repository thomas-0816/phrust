<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    if ($class === "PackBContract") {
        include __DIR__ . "/_data/PackBContract.php";
    }
    if ($class === "PackBTrait") {
        include __DIR__ . "/_data/PackBTrait.php";
    }
});

var_dump(interface_exists("PackBContract"));
var_dump(class_exists("PackBContract", false));
var_dump(trait_exists("PackBTrait"));
var_dump(class_exists("PackBTrait", false));
