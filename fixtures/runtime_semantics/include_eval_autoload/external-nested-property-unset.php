<?php

require __DIR__ . '/_data/external-nested-property-unset-child.php';

$probe = new ExternalNestedPropertyUnset();
$probe->sortValues();
var_dump($probe->removeNested());
var_dump($probe->values);

var_dump(external_nested_property_has());
var_dump(external_nested_property_remove());
var_dump(external_nested_property_has());
