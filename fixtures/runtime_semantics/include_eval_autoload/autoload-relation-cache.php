<?php
// runtime-semantics: expect=pass
spl_autoload_register(function ($class) {
    include (__DIR__ . "/_data/AutoloadRelationCacheChild.php");
});

$object = new AutoloadRelationCacheChild();
echo ($object instanceof AutoloadRelationCacheBase) ? "autoload-relation=yes\n" : "autoload-relation=no\n";
