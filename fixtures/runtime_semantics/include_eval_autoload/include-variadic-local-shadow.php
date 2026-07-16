<?php

$args = 'global-value';
require __DIR__ . '/_data/variadic-local-shadow-child.php';

var_dump(include_variadic_local_shadow('local-value'));
var_dump($args);
