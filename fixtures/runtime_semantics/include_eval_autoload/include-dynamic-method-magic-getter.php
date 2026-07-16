<?php

require __DIR__ . '/_data/dynamic-magic-child.php';

$object = new Fixture\Magic\DynamicMethodMagicGetter();
var_dump($object->computed, $object->value);
