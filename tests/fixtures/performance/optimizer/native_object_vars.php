<?php

class NativeObjectVarsBox
{
    public string $public = 'P';
    protected string $protected = 'R';
    private string $private = 'V';

    public function visibleFromClass(): string
    {
        $vars = get_object_vars($this);
        return $vars['public'] . $vars['protected'] . $vars['private'];
    }
}

function visible_object_vars(object $object): array
{
    return get_object_vars($object);
}

function mangled_object_vars(object $object): array
{
    return get_mangled_object_vars($object);
}

function native_object_class_name(object $object): string
{
    return get_class($object);
}

$box = new NativeObjectVarsBox();
$outside = visible_object_vars($box);
echo implode(',', array_keys($outside)), ':', implode(',', array_values($outside)), "\n";
echo $box->visibleFromClass(), "\n";
echo native_object_class_name($box), "\n";

foreach (mangled_object_vars($box) as $name => $value) {
    echo bin2hex((string) $name), '=', $value, "\n";
}

$dynamic = new stdClass();
$dynamic->first = 'A';
$dynamic->second = 'B';
$dynamicVars = visible_object_vars($dynamic);
echo implode(',', array_keys($dynamicVars)), ':', implode(',', array_values($dynamicVars)), "\n";
