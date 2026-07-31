<?php
// runtime-semantics: category=conversions expect=pass
// Exact native settype() family over authoritative local reference cells.

function nativeSettypeFamily(): array
{
    $integer = "42";
    $alias =& $integer;
    $integerResult = settype($integer, "Integer");

    $float = "3.5";
    $floatResult = settype($float, "double");

    $boolean = [];
    $booleanResult = settype($boolean, "boolean");

    $string = 17;
    $stringResult = settype($string, "string");

    $array = "native";
    $arrayResult = settype($array, "array");

    $object = "payload";
    $objectResult = settype($object, "object");

    $null = "discarded";
    $nullResult = settype($null, "null");

    return [
        $integerResult,
        $integer,
        $alias,
        $floatResult,
        $float,
        $booleanResult,
        $boolean,
        $stringResult,
        $string,
        $arrayResult,
        $array,
        $objectResult,
        $object instanceof stdClass,
        $object->scalar,
        $nullResult,
        $null,
    ];
}

var_dump(nativeSettypeFamily());

$invalid = 7;
try {
    settype($invalid, "native-unknown");
} catch (ValueError $error) {
    echo get_class($error), ": ", $error->getMessage(), "\n";
}
var_dump($invalid);

class SettypeDestructorBoundary
{
    public function __destruct()
    {
        echo "settype-destructor\n";
    }
}

function nativeSettypeDestructorBoundary(): void
{
    $value = new SettypeDestructorBoundary();
    var_dump(settype($value, "null"));
    var_dump($value);
}

nativeSettypeDestructorBoundary();
