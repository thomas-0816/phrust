<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    if ($class === "PackBInterfaceUser") {
        include __DIR__ . "/_data/PackBInterfaceUser.php";
    }
    if ($class === "PackBContract") {
        include __DIR__ . "/_data/PackBContract.php";
    }
});

$object = new PackBInterfaceUser();
echo $object->value(), "|";
var_dump($object instanceof PackBContract);
