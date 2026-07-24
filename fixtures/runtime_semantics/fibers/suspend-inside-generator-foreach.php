<?php

function suspended_leaf(): Generator {
    echo "leaf:start\n";
    $resume = Fiber::suspend("paused");
    echo "leaf:resume:$resume\n";
    yield "leaf" => $resume;
    return "leaf-return";
}

function delegating_generator(): Generator {
    $return = yield from suspended_leaf();
    yield "after" => $return;
}

$fiber = new Fiber(function (): string {
    foreach (delegating_generator() as $key => $value) {
        echo "$key=$value\n";
    }
    echo "fiber:end\n";
    return "fiber-return";
});

var_dump($fiber->start());
var_dump($fiber->resume("continued"));
var_dump($fiber->getReturn());
