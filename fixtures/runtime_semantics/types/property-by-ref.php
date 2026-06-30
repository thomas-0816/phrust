<?php
// runtime-semantics: category=types expect=pass
class Box {
    public int $value;
}

$box = new Box();
$box->value = 1;
$ref =& $box->value;
$ref = 2;
echo $box->value, "\n";
