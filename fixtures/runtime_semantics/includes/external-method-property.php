<?php

require __DIR__ . '/../_data/external-method-property.php';

$value = new ExternalMethodProperty();
var_dump($value->get());
