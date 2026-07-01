<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    if ($class === "PackBExists") {
        include __DIR__ . "/_data/PackBExists.php";
    }
});

$object = new PackBExists();
echo get_class($object), "\n";
