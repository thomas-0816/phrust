<?php

interface NativeRootContract {}
interface NativeChildContract extends NativeRootContract {}

class NativeRootClass {}
class NativeMiddleClass extends NativeRootClass implements NativeChildContract {}
class NativeLeafClass extends NativeMiddleClass {}

function native_parent_class($value): string|false
{
    return get_parent_class($value);
}

function native_subclass($value, string $parent, bool $allowString = true): bool
{
    return is_subclass_of($value, $parent, $allowString);
}

$leaf = new NativeLeafClass();
echo native_parent_class($leaf), "\n";
echo native_parent_class('NativeLeafClass'), "\n";
echo native_subclass($leaf, 'NativeRootClass', false) ? "object-parent\n" : "bad\n";
echo native_subclass('NativeLeafClass', 'NativeRootClass') ? "string-parent\n" : "bad\n";
echo native_subclass('NativeLeafClass', 'NativeRootClass', false) ? "bad\n" : "string-disabled\n";
echo native_subclass($leaf, 'NativeRootContract') ? "interface-parent\n" : "bad\n";
echo native_subclass('NativeRootClass', 'NativeRootClass') ? "bad\n" : "not-self\n";
echo native_subclass('MissingNativeClass', 'NativeRootClass') ? "bad\n" : "missing\n";
