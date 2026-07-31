<?php

class NativeArrayCastFixture
{
    public string $public = "public";
    protected string $protected = "protected";
    private string $private = "private";
}

function native_array_casts(): array
{
    $nothing = null;
    $integer = 7;
    $float = 2.5;
    $string = "native";
    $source = [1, 2];

    $empty = (array) $nothing;
    $integerArray = (array) $integer;
    $floatArray = (array) $float;
    $stringArray = (array) $string;
    $copy = (array) $source;
    $copy[] = 3;

    $object = new NativeArrayCastFixture();
    $objectArray = (array) $object;
    $numericObject = new stdClass();
    $numericObject->{7} = "seven";
    $numericArray = (array) $numericObject;

    return [
        $empty,
        $integerArray,
        $floatArray,
        $stringArray,
        $source,
        $copy,
        $objectArray["public"],
        $objectArray["\0*\0protected"],
        $objectArray["\0NativeArrayCastFixture\0private"],
        $numericArray[7],
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_array_casts();
}
var_dump($result);
