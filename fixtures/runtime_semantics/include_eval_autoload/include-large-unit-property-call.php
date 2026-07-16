<?php
// runtime-semantics: category=include_eval_autoload expect=pass
require __DIR__ . '/_data/large-unit-property-call-child.php';

$state = new LargeUnitPropertyCall('database.test');
var_dump($state->connect());
var_dump($state->callOptional());
