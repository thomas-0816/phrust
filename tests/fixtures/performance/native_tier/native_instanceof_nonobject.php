<?php

class NativeInstanceofMarker
{
}

function is_native_instanceof_marker($value)
{
    return $value instanceof NativeInstanceofMarker;
}

echo is_native_instanceof_marker(null), is_native_instanceof_marker(false), "\n";
