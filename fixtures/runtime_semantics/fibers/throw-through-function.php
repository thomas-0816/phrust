<?php
function suspend_for_throw(): void
{
    Fiber::suspend("ready");
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
