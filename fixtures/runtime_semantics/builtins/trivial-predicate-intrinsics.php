<?php
// Intrinsic fast paths for trivial predicates and array-pointer reads must
// match the generic builtins exactly: closures/generators count as objects,
// references dereference, and current/key honor the internal pointer.

function gen() { yield 1; }

class NativeCountablePredicate implements Countable
{
    public function count(): int { return 1; }
}

class NativeIterablePredicate implements IteratorAggregate
{
    public function getIterator(): Traversable { yield 1; }
}

$cases = [
    "null" => null,
    "false" => false,
    "int" => 7,
    "float" => 1.5,
    "string" => "s",
    "int_string" => "42",
    "float_string" => "3.14",
    "leading_numeric" => "12abc",
    "array" => [1],
    "object" => new stdClass(),
    "closure" => function () {},
    "generator" => gen(),
    "countable" => new NativeCountablePredicate(),
    "iterable" => new NativeIterablePredicate(),
];
foreach ($cases as $label => $value) {
    echo $label, ":";
    echo is_object($value) ? "O" : "-";
    echo is_null($value) ? "N" : "-";
    echo is_scalar($value) ? "S" : "-";
    echo is_bool($value) ? "B" : "-";
    echo is_float($value) ? "F" : "-";
    echo is_numeric($value) ? "M" : "-";
    echo is_countable($value) ? "C" : "-";
    echo is_iterable($value) ? "I" : "-";
    echo "\n";
}

// Reference arguments keep dereferencing.
$value = 2.25;
$ref = &$value;
var_dump(is_float($ref), is_object($ref));

// current/key follow the internal pointer, including past-the-end.
$arr = ["x" => 10, "y" => 20];
var_dump(current($arr), key($arr));
next($arr);
var_dump(current($arr), key($arr));
next($arr);
var_dump(current($arr), key($arr));
reset($arr);
var_dump(current($arr), key($arr));

// Empty arrays report false/null.
$empty = [];
var_dump(current($empty), key($empty));
