<?php
require __DIR__ . '/define-cross-unit-reader.inc';
var_dump(define('CROSS_UNIT_NATIVE_FLOAT', 2.5));
var_dump(read_cross_unit_native_constant());
