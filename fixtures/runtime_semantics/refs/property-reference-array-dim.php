<?php
// runtime-semantics: expect=pass
class PropertyReferenceArrayBox {
    public array $items = ["a" => 1];
}

$box = new PropertyReferenceArrayBox();
$alias =& $box->items;
$alias["b"] = 2;
echo $box->items["a"], "|", $box->items["b"];
