<?php
// runtime-semantics: category=callables expect=pass php_ref_required=1

function native_callable_named($value)
{
    return $value;
}

class NativeCallableTarget
{
    public static function staticMethod()
    {
        return 1;
    }

    public function instanceMethod()
    {
        return 2;
    }

    public function __invoke()
    {
        return 3;
    }

    public function __call($name, $arguments)
    {
        return 4;
    }
}

function native_callable_query_boundary($candidate, $syntaxOnly)
{
    $name = "unchanged";
    $result = is_callable($candidate, $syntaxOnly, $name);
    return [$result, $name];
}

$target = new NativeCallableTarget();
$closure = static function () {
    return 5;
};
for ($round = 0; $round < 32; $round++) {
    $named = native_callable_query_boundary("native_callable_named", false);
    $builtin = native_callable_query_boundary("strlen", false);
    $staticString = native_callable_query_boundary(
        "NativeCallableTarget::staticMethod",
        false
    );
    $staticArray = native_callable_query_boundary(
        ["NativeCallableTarget", "staticMethod"],
        false
    );
    $objectMethod = native_callable_query_boundary(
        [$target, "instanceMethod"],
        false
    );
    $magicMethod = native_callable_query_boundary(
        [$target, "missingMethod"],
        false
    );
    $invokable = native_callable_query_boundary($target, false);
    $closureResult = native_callable_query_boundary($closure, false);
    $missing = native_callable_query_boundary(
        "MissingCallableTarget::method",
        false
    );
    $syntaxOnly = native_callable_query_boundary(
        ["MissingCallableTarget", "method"],
        true
    );
    $scalar = native_callable_query_boundary(42, false);
}

var_dump($named);
var_dump($builtin);
var_dump($staticString);
var_dump($staticArray);
var_dump($objectMethod);
var_dump($magicMethod);
var_dump($invokable);
var_dump($closureResult);
var_dump($missing);
var_dump($syntaxOnly);
var_dump($scalar);
