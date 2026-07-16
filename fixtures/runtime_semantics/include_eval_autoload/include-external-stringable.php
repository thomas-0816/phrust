<?php

require __DIR__ . '/_data/stringable-child.php';

$value = new Fixture\Stringable\ExternalStringable();
var_dump((string) $value);
