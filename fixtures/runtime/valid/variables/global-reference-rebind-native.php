<?php

function bind_native_global_reference(&$source): void
{
    $native_global_alias = 5;
    $GLOBALS['native_global_alias'] =& $source;
    echo $native_global_alias, "\n";
}

require __DIR__ . '/global-reference-rebind-native-target.inc';

$native_global_alias = 3;
$source = 7;
bind_native_global_reference($source);
$source = 9;
echo $native_global_alias, "\n";
$native_global_alias = 11;
echo $source, "\n";

$cross_unit_native_global_alias = 13;
$cross_unit_source = 17;
bind_cross_unit_native_global_reference($cross_unit_source);
$cross_unit_source = 19;
echo $cross_unit_native_global_alias, "\n";
$cross_unit_native_global_alias = 23;
echo $cross_unit_source, "\n";
