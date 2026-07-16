<?php
// runtime-semantics: expect=pass
class PropertyDimensionReferenceBox {
    public array $items = ["a" => 1];
}

$box = new PropertyDimensionReferenceBox();
$value = 7;
$box->items["a"] =& $value;
$value = 9;
echo $box->items["a"], "|", $value;
