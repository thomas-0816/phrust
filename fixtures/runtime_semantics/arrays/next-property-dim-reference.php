<?php

class PointerHolder
{
    public array $iterations = [[10, 20, 30]];

    public function advance(): mixed
    {
        return next($this->iterations[0]);
    }

    public function current(): mixed
    {
        return current($this->iterations[0]);
    }
}

$holder = new PointerHolder();
var_dump($holder->current());
var_dump($holder->advance());
var_dump($holder->current());
var_dump($holder->advance());
