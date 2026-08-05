<?php

function invoke_cross_unit_native_global_binder(object $binder, &$source): void
{
    $binder->bind($source);
}

function construct_cross_unit_native_global_binder(string $class): CrossUnitNativeGlobalBinder
{
    return new $class();
}

require __DIR__ . '/global-reference-rebind-method-target.inc';

$cross_unit_method_global_alias = 43;
$cross_unit_method_source = 45;
$cross_unit_method_binder = construct_cross_unit_native_global_binder(
    CrossUnitNativeGlobalBinder::class,
);
invoke_cross_unit_native_global_binder($cross_unit_method_binder, $cross_unit_method_source);
$cross_unit_method_source = 47;
echo $cross_unit_method_global_alias, "\n";
$cross_unit_method_global_alias = 53;
echo $cross_unit_method_source, "\n";
