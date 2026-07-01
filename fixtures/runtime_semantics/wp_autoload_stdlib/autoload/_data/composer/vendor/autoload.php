<?php
$classMap = [
    "PackBComposer\\ClassMapService" => __DIR__ . "/../src/ClassMapService.php",
];

spl_autoload_register(function ($class) use ($classMap) {
    if (isset($classMap[$class])) {
        include $classMap[$class];
    }
});
