<?php
// runtime-semantics: category=calls expect=pass

function native_callable_increment(int $value): int
{
    return $value + 1;
}

function native_callable_untyped($value)
{
    return $value + 2;
}

function native_callable_string(string $value): string
{
    return $value . '!';
}

function native_callable_array(array $value): array
{
    $value[] = 'callee';
    return $value;
}

final class NativeCallableViewTarget
{
    public function __construct(private int $delta)
    {
    }

    public function add(int $value): int
    {
        return $value + $this->delta;
    }

    public static function twice(int $value): int
    {
        return $value * 2;
    }

    public function __invoke(int $value): int
    {
        return $value - $this->delta;
    }

    public function makeAdder(int $captured): Closure
    {
        return fn (int $value): int => $value + $this->delta + $captured;
    }
}

function native_invoke_prepared(callable $callback, int $value): int
{
    return $callback($value);
}

function native_invoke_prepared_raw(callable $callback, mixed $value): mixed
{
    return $callback($value);
}

function native_invoke_prepared_array(callable $callback, array $arguments): mixed
{
    return call_user_func_array($callback, $arguments);
}

function native_invoke_prepared_owned(callable $callback): array
{
    return $callback(['fixed']);
}

function native_invoke_prepared_owned_unpack(callable $callback): array
{
    return call_user_func_array($callback, [['unpack']]);
}

function native_prepared_callable_type_failure(): string
{
    try {
        native_invoke_prepared_raw('native_callable_increment', []);
    } catch (TypeError $error) {
        return $error::class;
    }

    return 'missing TypeError';
}

function native_prepared_callable_family(): array
{
    $target = new NativeCallableViewTarget(4);
    $closure = static fn (int $value): int => $value + 3;
    $captured = 7;
    $capturingClosure = static fn (string $value): string => $value . ':' . $captured;
    $boundCapturingClosure = $target->makeAdder(6);

    return [
        native_invoke_prepared('native_callable_increment', 5),
        native_invoke_prepared('native_callable_untyped', 5),
        native_invoke_prepared_raw('native_callable_string', 7),
        native_invoke_prepared_array('native_callable_increment', [5]),
        native_invoke_prepared_array('native_callable_string', [8]),
        native_invoke_prepared_owned('native_callable_array'),
        native_invoke_prepared_owned_unpack('native_callable_array'),
        native_invoke_prepared_array('native_callable_increment', ['value' => 9]),
        native_invoke_prepared('abs', -5),
        native_invoke_prepared([$target, 'add'], 5),
        native_invoke_prepared_raw([$target, 'add'], '5'),
        native_invoke_prepared_array([$target, 'add'], ['6']),
        native_invoke_prepared([NativeCallableViewTarget::class, 'twice'], 5),
        native_invoke_prepared_raw([NativeCallableViewTarget::class, 'twice'], '5'),
        native_invoke_prepared_array([NativeCallableViewTarget::class, 'twice'], ['6']),
        native_invoke_prepared($closure, 5),
        native_invoke_prepared_raw($capturingClosure, 9),
        native_invoke_prepared($boundCapturingClosure, 5),
        native_invoke_prepared_array($boundCapturingClosure, [8]),
        native_invoke_prepared($target, 5),
        native_invoke_prepared_raw($target, '5'),
        spl_object_id($closure) === spl_object_id($closure),
        native_prepared_callable_type_failure(),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_prepared_callable_family();
}

var_dump($result);
