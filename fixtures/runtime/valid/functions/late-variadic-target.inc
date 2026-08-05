<?php
function late_variadic_native(string $head, int ...$tail): string
{
    if (count($tail) === 0) {
        return $head . '|0';
    }
    return $head . '|' . count($tail) . '|' . $tail[0] . '|' . $tail[2];
}
