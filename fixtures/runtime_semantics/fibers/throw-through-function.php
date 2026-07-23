<?php
function suspend_inner_for_throw(): void
{
    Fiber::suspend("ready");
}

function suspend_for_throw(): void
{
    suspend_inner_for_throw();
}

$fiber = new Fiber(function (): string {
    try {
        suspend_for_throw();
        echo "missed\n";
    } catch (RuntimeException $exception) {
        echo "caught:", $exception->getMessage(), "\n";
    }

    return "done";
});

var_dump($fiber->start());
var_dump($fiber->throw(new RuntimeException("boom")));
var_dump($fiber->getReturn());
