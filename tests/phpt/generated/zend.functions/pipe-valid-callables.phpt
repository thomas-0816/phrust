--TEST--
Generated zend.functions: pipe RHS supports callable forms
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: pipe RHS dispatch uses unified callable acquisition for function, closure, and builtin callables
--FILE--
<?php
function prompt13_pipe_plus_one($value) {
    return $value + 1;
}

$closure = fn($value) => $value + 2;

echo 2 |> prompt13_pipe_plus_one(...), "\n";
echo 2 |> $closure, "\n";
echo " hi " |> trim(...), "\n";
?>
--EXPECT--
3
4
hi
