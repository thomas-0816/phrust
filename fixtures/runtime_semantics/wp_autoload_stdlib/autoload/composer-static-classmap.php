<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
require __DIR__ . "/_data/composer/vendor/autoload.php";

echo PackBComposer\ClassMapService::label(), "|";
var_dump(class_exists("PackBComposer\\ClassMapService", false));
