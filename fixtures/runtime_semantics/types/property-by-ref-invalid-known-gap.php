<?php
// runtime-semantics: category=types expect=known_gap known_gap=E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE
class TypedPropertyReferenceInvalidBox {
    public int $value = 1;
}

$box = new TypedPropertyReferenceInvalidBox();
$alias =& $box->value;
$alias = "not-an-int";
echo $box->value, "\n";
