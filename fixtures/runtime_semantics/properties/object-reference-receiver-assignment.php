<?php

class ObjectReferenceReceiverBox
{
    public int $value = 1;
}

$object = new ObjectReferenceReceiverBox();
$alias =& $object;
$alias->value = 7;

var_dump($object->value);
var_dump($alias === $object);
