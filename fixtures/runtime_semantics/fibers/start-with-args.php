<?php
$fiber = new Fiber(function ($a, $b): void {
    echo $a + $b, "\n";
});

$fiber->start(2, 5);
var_dump($fiber->isTerminated());

function run_fiber_cow(): void
{
    $source = ['value' => 'caller'];
    $cow = new Fiber(function ($payload, $suffix) {
        $payload['value'] .= $suffix;
        return $payload;
    });

    var_dump($cow->start($source, '-fiber'));
    var_dump($source);
    var_dump($cow->getReturn());
}

run_fiber_cow();
