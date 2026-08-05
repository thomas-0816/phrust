<?php

class NativeDynamicVariadicTarget
{
    public function collect(string $head, string ...$tail): string
    {
        return $head . '|' . implode('|', $tail);
    }
}

function invoke_native_dynamic_variadic(object $target): string
{
    return $target->collect('a', 'b', 'c');
}

echo invoke_native_dynamic_variadic(new NativeDynamicVariadicTarget()), "\n";
