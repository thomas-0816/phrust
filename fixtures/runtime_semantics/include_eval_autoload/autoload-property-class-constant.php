<?php
// runtime-semantics: category=include_eval_autoload expect=pass
spl_autoload_register(function (string $class): void {
    require __DIR__ . '/_data/property_class_constant/' . $class . '.php';
});

require __DIR__ . '/_data/property_class_constant/RequestDefaults.php';

$defaults = new RequestDefaults();
echo $defaults->normalization['port'], "\n";
