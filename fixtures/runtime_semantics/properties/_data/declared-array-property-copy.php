<?php

class DeclaredArrayPropertyCopy
{
    public $parsed;

    public function __construct($parsed)
    {
        $this->parsed = $parsed;
    }

    public function mutateCopy()
    {
        $copy = $this->parsed;
        $copy['name'] = 'copy';
        return array($this->parsed, $copy);
    }
}

function mutate_declared_array_property_copy($holder)
{
    $copy = $holder->parsed;
    $copy['name'] = 'copy';
    return array($holder->parsed, $copy);
}

function consume_declared_array_property_children($children)
{
    return count($children);
}

class NestedDeclaredArrayPropertyCopy
{
    public $parsed;

    public function __construct($parsed)
    {
        $this->parsed = $parsed;
        if (!empty($this->parsed['children'])) {
            consume_declared_array_property_children($this->parsed['children']);
        }
    }
}
