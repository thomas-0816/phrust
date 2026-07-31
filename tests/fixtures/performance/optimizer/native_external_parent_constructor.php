<?php

require_once __DIR__ . '/native_external_parent_constructor.inc';

class NativeExternalParentConstructorChild extends NativeExternalParentConstructorBase
{
    public string $label = 'child';
}

function native_external_parent_constructor_case(): void
{
    $first = new NativeExternalParentConstructorChild(7);
    $second = new NativeExternalParentConstructorChild();

    echo get_class($first), ':', $first->count, ':', $first->label, "\n";
    echo get_class($second), ':', $second->count, ':', $second->label, "\n";
}

native_external_parent_constructor_case();
