<?php
// runtime-semantics: category=conversions expect=pass php_ref_required=1

function native_object_cast($value)
{
    return (object) $value;
}

$empty = native_object_cast(null);
echo isset($empty->scalar) ? "bad-empty" : "empty", "|";

$integer = native_object_cast(42);
$string = native_object_cast("text");
$boolean = native_object_cast(true);
echo $integer->scalar, "|", $string->scalar, "|", $boolean->scalar ? "true" : "false", "|";

$array = native_object_cast([
    "first" => "A",
    7 => "B",
    "last" => ["nested"],
]);
echo $array->first, "|", $array->{7}, "|", $array->last[0], "|";
foreach ($array as $name => $value) {
    echo $name, ",";
}
echo "|";

$original = new stdClass();
$original->state = "before";
$same = native_object_cast($original);
$same->state = "after";
echo $original->state, "|", $same === $original ? "same" : "different", "|";

$referenced = 17;
$alias =& $referenced;
$reference_object = native_object_cast($alias);
echo $reference_object->scalar, "\n";
