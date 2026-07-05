<?php
// runtime-semantics: category=known_gaps expect=known_gap known_gap=E_PHP_IR_PIPE_CALL_INPUT_GAP
// PHP reference: any expression, including a call result, can feed the pipe
// operator's input side.
function double(int $n): int
{
    return $n * 2;
}

$x = strlen('abcd') |> double(...);
var_dump($x);
