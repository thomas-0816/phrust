<?php
// runtime-semantics: expect=pass

class ExternalPropertyArgumentsHolder
{
    public string $first = 'first';
    public string $second = 'second';
}

$holder = new ExternalPropertyArgumentsHolder();
include __DIR__ . '/_data/external-property-arguments-child.php';

external_property_arguments($holder->first, $holder->second);
