<?php

function suspend_callback(string $label): string
{
    $value = Fiber::suspend($label);
    return $label . ':' . $value;
}

function suspend_through_call_user_func(): string
{
    return call_user_func('suspend_callback', 'direct');
}

function suspend_through_call_user_func_array(): string
{
    return call_user_func_array('suspend_callback', ['array']);
}

$direct = new Fiber(static fn(): string => suspend_through_call_user_func());
var_dump($direct->start());
var_dump($direct->resume('resumed'));
var_dump($direct->getReturn());

$array = new Fiber(static fn(): string => suspend_through_call_user_func_array());
var_dump($array->start());
var_dump($array->resume('resumed'));
var_dump($array->getReturn());
