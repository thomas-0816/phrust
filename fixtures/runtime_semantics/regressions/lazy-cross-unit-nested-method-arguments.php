<?php
// runtime-semantics: expect=pass regression_category=objects reference_behavior=stdout:first regression_case=lazy-native-compilation

require __DIR__ . '/_data/lazy-cross-unit-nested-method-class.php';
require __DIR__ . '/_data/lazy-cross-unit-nested-method-function.php';
require __DIR__ . '/_data/lazy-cross-unit-nested-method-callback.php';

$lazy_cross_unit_hooks = array(
    'ready' => new LazyCrossUnitNestedMethodHook(),
);

echo lazy_cross_unit_nested_method_action('ready', 'first'), "\n";
