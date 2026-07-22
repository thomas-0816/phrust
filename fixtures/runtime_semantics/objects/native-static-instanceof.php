<?php

interface NativeInstanceMarker {}

class NativeInstanceRoot {}

class NativeInstanceChild extends NativeInstanceRoot implements NativeInstanceMarker {}

class NativeInstanceOther {}

function native_static_instanceof(mixed $value): void
{
    var_dump($value instanceof NativeInstanceChild);
    var_dump($value instanceof NativeInstanceRoot);
    var_dump($value instanceof NativeInstanceMarker);
    var_dump($value instanceof NativeInstanceOther);
}

$child = new NativeInstanceChild();
native_static_instanceof($child);

$alias =& $child;
native_static_instanceof($alias);

native_static_instanceof(new NativeInstanceOther());
native_static_instanceof(17);
