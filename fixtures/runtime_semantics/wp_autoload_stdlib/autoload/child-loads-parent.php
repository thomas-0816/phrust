<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    if ($class === "PackBChild") {
        include __DIR__ . "/_data/PackBChild.php";
    }
    if ($class === "PackBParent") {
        include __DIR__ . "/_data/PackBParent.php";
    }
});

$child = new PackBChild();
echo $child->parentValue(), "|";
var_dump($child instanceof PackBParent);
