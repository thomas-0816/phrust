<?php
require __DIR__ . "/_data/composer_static/vendor/autoload.php";

echo ComposerStatic\Service::NAME, "|";
echo ComposerStatic\Service::label(), "|";
echo ComposerMapped\Thing::$label, "\n";
