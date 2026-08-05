<?php

function generated_shadow_inner(int $number, string ...$tail): void
{
    $trace = debug_backtrace(0, 2);
    echo $trace[0]['function'], '|', count($trace[0]['args']), '|';
    echo $trace[0]['args'][0], '|', $trace[0]['args'][1], "\n";
    echo $trace[1]['function'], '|', count($trace[1]['args']), "\n";
}

function generated_shadow_outer(): void
{
    generated_shadow_inner(7, 'tail');
}

generated_shadow_outer();
