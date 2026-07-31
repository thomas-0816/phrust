<?php

const NATIVE_UNIT_INT = 17;
const NATIVE_UNIT_ARRAY = ['alpha' => 1, 4 => 'four'];

define('NATIVE_DEFINE_STRING', 'native');
define('NATIVE_DEFINE_ARRAY', ['nested' => [2, 3]]);

$flat = get_defined_constants();
var_dump($flat['PHP_VERSION'] === PHP_VERSION);
var_dump($flat['NATIVE_UNIT_INT'] === 17);
var_dump($flat['NATIVE_UNIT_ARRAY'] === ['alpha' => 1, 4 => 'four']);
var_dump($flat['NATIVE_DEFINE_STRING'] === 'native');
var_dump($flat['NATIVE_DEFINE_ARRAY'] === ['nested' => [2, 3]]);
var_dump(
    is_resource($flat['STDIN'])
    && is_resource($flat['STDOUT'])
    && is_resource($flat['STDERR'])
);

$categorized = get_defined_constants(true);
var_dump($categorized['Core']['PHP_VERSION'] === PHP_VERSION);
var_dump($categorized['json']['JSON_ERROR_NONE'] === JSON_ERROR_NONE);
var_dump($categorized['user']['NATIVE_UNIT_INT'] === 17);
var_dump($categorized['user']['NATIVE_DEFINE_ARRAY'] === ['nested' => [2, 3]]);
var_dump(is_resource($categorized['Core']['STDIN']));

include __DIR__ . '/native_constant_inventory.inc';

$after = get_defined_constants();
var_dump($after['NATIVE_INCLUDED_CONST'] === 'included');
var_dump($after['NATIVE_INCLUDED_DEFINE'] === ['boundary' => [5, 8]]);

$afterCategorized = get_defined_constants(true);
var_dump($afterCategorized['user']['NATIVE_INCLUDED_CONST'] === 'included');
var_dump($afterCategorized['user']['NATIVE_INCLUDED_DEFINE'] === ['boundary' => [5, 8]]);
