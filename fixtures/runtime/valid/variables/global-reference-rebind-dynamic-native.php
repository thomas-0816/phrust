<?php

function invoke_native_global_binder(string $binder, &$source): void
{
    $binder($source);
}

require __DIR__ . '/global-reference-rebind-native-target.inc';

$cross_unit_native_global_alias = 29;
$dynamic_cross_unit_source = 31;
invoke_native_global_binder(
    'bind_cross_unit_native_global_reference',
    $dynamic_cross_unit_source,
);
$dynamic_cross_unit_source = 37;
echo $cross_unit_native_global_alias, "\n";
$cross_unit_native_global_alias = 41;
echo $dynamic_cross_unit_source, "\n";
