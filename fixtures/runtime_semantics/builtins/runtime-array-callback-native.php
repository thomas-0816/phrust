<?php
// runtime-semantics: category=callables expect=pass

function runtime_array_map(callable $callback, array $values): array
{
    return array_map($callback, $values);
}

function runtime_array_filter(callable $callback, array $values): array
{
    return array_filter($values, $callback);
}

function runtime_array_reduce(callable $callback, array $values, mixed $initial): mixed
{
    return array_reduce($values, $callback, $initial);
}

function runtime_array_walk(callable $callback, array $values, int $delta): array
{
    $copy = $values;
    $result = array_walk($values, $callback, $delta);
    return [$result, $values, $copy];
}

function runtime_array_walk_recursive(callable $callback, array $values, int $delta): array
{
    $copy = $values;
    $result = array_walk_recursive($values, $callback, $delta);
    return [$result, $values, $copy];
}

function runtime_array_walk_export(callable $callback, array $values, int $delta): array
{
    global $runtime_walk_alias;
    $runtime_walk_alias = null;
    $copy = $values;
    $result = array_walk($values, $callback, $delta);
    $runtime_walk_alias += 100;
    return [$result, $values, $copy, $runtime_walk_alias];
}

function runtime_array_increment(int $value): int
{
    return $value + 1;
}

function runtime_array_odd(int $value): bool
{
    return ($value & 1) !== 0;
}

function runtime_array_sum(int $carry, int $value): int
{
    return $carry + $value;
}

function runtime_array_mutate(int &$value, $key, int $delta): void
{
    $value += $delta + (is_int($key) ? $key : strlen($key));
}

function runtime_array_observe(int $value, $key, int $delta): void
{
}

function runtime_array_observe_reference(&$value, $key, int $delta): void
{
}

function runtime_array_export_reference(&$value, $key, int $delta): void
{
    global $runtime_walk_alias;
    $runtime_walk_alias =& $value;
    $value += $delta;
}

final class RuntimeArrayCallbackTarget
{
    public function __construct(private int $delta)
    {
    }

    public function add(int $value): int
    {
        return $value + $this->delta;
    }

    public function mutate(int &$value, $key, int $delta): void
    {
        $value += $this->delta + $delta + (is_int($key) ? $key : strlen($key));
    }
}

$target = new RuntimeArrayCallbackTarget(5);
$captured = 7;
$closure = static fn (int $value): int => $value + $captured;
$walkClosure = static function (int &$value, $key, int $delta) use ($captured): void {
    $value += $captured + $delta + (is_int($key) ? $key : strlen($key));
};
$result = null;

for ($iteration = 0; $iteration < 64; $iteration++) {
    $result = [
        runtime_array_map('runtime_array_increment', ['1', 2, 3]),
        runtime_array_map([$target, 'add'], [1, 2, 3]),
        runtime_array_map($closure, [1, 2, 3]),
        runtime_array_filter('runtime_array_odd', [1, 2, 3, 4]),
        runtime_array_reduce('runtime_array_sum', [1, 2, 3, 4], 10),
        runtime_array_walk('runtime_array_mutate', [2 => 10, 'abc' => 20], 3),
        runtime_array_walk('runtime_array_observe', [2 => 10, 'abc' => 20], 3),
        runtime_array_walk('runtime_array_observe_reference', [2 => 10, 'abc' => 20], 3),
        runtime_array_walk($walkClosure, [2 => 10, 'abc' => 20], 3),
        runtime_array_walk_export(
            'runtime_array_export_reference',
            [2 => 10, 'abc' => 20],
            3,
        ),
        runtime_array_walk_recursive(
            [$target, 'mutate'],
            [2 => 10, 4 => [6 => 20, 'abc' => 30]],
            3,
        ),
    ];
}

var_dump($result);
