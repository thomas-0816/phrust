<?php
#[\NoDiscard]
function matrix_value(): int {
    return 1;
}

$result = "hello" |> strtoupper(...) |> strlen(...);
$copy = clone(new class {
    public string $name = "old";
}, ["name" => "new"]);
(void) matrix_value();

class Php85Matrix {
    public const VALUE = (int) "42";
    public const FACTORY = static function (): int {
        return 1;
    };
}
