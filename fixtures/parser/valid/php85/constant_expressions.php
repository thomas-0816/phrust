<?php

#[\NoDiscard]
function php85_result(): int {
    return 1;
}

class Php85ConstantExpressions {
    public const CAST_VALUE = (int) "42";
    public const CALLABLE_VALUE = strlen(...);
    public const CLOSURE_VALUE = static function (): int {
        return 1;
    };
}
