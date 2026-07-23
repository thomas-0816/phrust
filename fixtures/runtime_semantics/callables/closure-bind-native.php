<?php
// runtime-semantics: category=callables expect=pass php_ref_required=1

class ClosureBindFixtureSecret
{
    private int $value = 10;
}

function makeClosureBindFixture(int $captured): Closure
{
    return function () use ($captured) {
        return $this->value + $captured;
    };
}

function makeSuspendingClosureBindFixture(int $captured): Closure
{
    return function () use ($captured) {
        $resume = Fiber::suspend($this->value);

        return $resume + $captured;
    };
}

$closure = makeClosureBindFixture(5);
$target = new ClosureBindFixtureSecret();
$bound = Closure::bind($closure, $target, ClosureBindFixtureSecret::class);
$boundTo = $closure->bindTo($target, ClosureBindFixtureSecret::class);

echo $bound(), "\n";
echo $boundTo(), "\n";
echo spl_object_id($closure) === spl_object_id($bound) ? "same\n" : "distinct\n";

$suspending = makeSuspendingClosureBindFixture(5);
$boundSuspending = $suspending->bindTo($target, ClosureBindFixtureSecret::class);
$fiber = new Fiber($boundSuspending);
var_dump($fiber->start());
var_dump($fiber->resume(7));
var_dump($fiber->getReturn());
