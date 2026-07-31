<?php

require_once __DIR__ . '/native_external_parent_object.inc';

class NativeExternalParentObjectChild extends NativeExternalParentObjectBase
{
    public string $label = 'child';
}

function native_external_parent_object_case(): void
{
    $first = new NativeExternalParentObjectChild();
    $first->items['seed']['value']++;
    echo get_class($first), ':', $first->items['seed']['value'], ':',
        $first->count, ':', $first->label, "\n";

    $second = new NativeExternalParentObjectChild();
    echo get_class($second), ':', $second->items['seed']['value'], ':',
        $second->count, ':', $second->label, "\n";
}

native_external_parent_object_case();
