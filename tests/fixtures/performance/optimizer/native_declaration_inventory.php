<?php

function native_inventory_function(): void
{
}

interface NativeInventoryInterface
{
}

trait NativeInventoryTrait
{
}

class NativeInventoryClass implements NativeInventoryInterface
{
    use NativeInventoryTrait;
}

$functions = get_defined_functions();
$classes = get_declared_classes();
$interfaces = get_declared_interfaces();
$traits = get_declared_traits();

var_dump(isset($functions['internal'], $functions['user']));
var_dump(in_array('native_inventory_function', $functions['user'], true));
var_dump(in_array('NativeInventoryClass', $classes, true));
var_dump(in_array('NativeInventoryInterface', $interfaces, true));
var_dump(in_array('NativeInventoryTrait', $traits, true));
var_dump(!in_array('NativeInventoryInterface', $classes, true));
var_dump(!in_array('NativeInventoryTrait', $classes, true));
