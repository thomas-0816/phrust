<?php

final class ExternalNestedPropertyUnset
{
    public $values = array(
        10 => array('keep' => true, 'remove' => true),
    );

    public function removeNested(int $priority = 10, string $key = 'remove'): bool
    {
        $exists = isset($this->values[$priority][$key]);
        unset($this->values[$priority][$key]);
        return $exists;
    }

    public function sortValues(): void
    {
        ksort($this->values);
    }
}

$external_nested_property_unset = array(
    'hook' => new ExternalNestedPropertyUnset(),
);
$external_nested_property_unset['hook']->sortValues();

function external_nested_property_remove(): bool
{
    global $external_nested_property_unset;

    $priority = 10;
    $key = 'remove';
    $removed = $external_nested_property_unset['hook']->removeNested($priority, $key);
    if (!$external_nested_property_unset['hook']->values[10]) {
        unset($external_nested_property_unset['hook']->values[10]);
    }
    return $removed;
}

function external_nested_property_has(): bool
{
    global $external_nested_property_unset;

    return isset($external_nested_property_unset['hook']->values[10]['remove']);
}
