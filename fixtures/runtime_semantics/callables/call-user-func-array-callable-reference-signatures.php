<?php

set_error_handler(static function (int $level, string $message): bool {
    echo "warning:$message\n";
    return true;
});

function direct_reference_function(&$value): void
{
    $value++;
    echo "function:$value\n";
}

final class DirectReferenceCallable
{
    public static function staticMethod(&$value): void
    {
        $value++;
        echo "static:$value\n";
    }

    public function instanceMethod(&$value): void
    {
        $value++;
        echo "instance:$value\n";
    }

    public function __invoke(&$value): void
    {
        $value++;
        echo "invoke:$value\n";
    }
}

$object = new DirectReferenceCallable();
$closure = function (&$value): void {
    $value++;
    echo "closure:$value\n";
};

foreach ([
    "direct_reference_function",
    [DirectReferenceCallable::class, "staticMethod"],
    [$object, "instanceMethod"],
    $object,
    $closure,
] as $callback) {
    call_user_func_array($callback, [1]);
}
