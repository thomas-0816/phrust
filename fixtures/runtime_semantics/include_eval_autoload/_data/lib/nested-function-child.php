<?php

class NestedIncludeExportedObject
{
    public function main(string $value): string
    {
        $values = [$value];
        $alias =& $values;

        return array_merge($alias, [])[0];
    }
}

$GLOBALS['nested_include_exported_object'] = new NestedIncludeExportedObject();

function nested_include_exported_function(string $value = 'nested-export-ok'): string
{
    global $nested_include_exported_object;

    $server = array_merge(['nested-value' => $value], $_SERVER);

    return $nested_include_exported_object->main($server['nested-value']);
}
