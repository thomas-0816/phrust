<?php

class MethodArgumentOrderFixture {
    public function combine(string $first, string $second = 'default'): string {
        return $first . '|' . $second;
    }
}

$fixture = new MethodArgumentOrderFixture();
var_dump($fixture->combine('first', 'second'));
var_dump($fixture->combine('only'));

$method = [$fixture, 'combine'];
var_dump($method('callable-first', 'callable-second'));

function invokeDynamicMethod(object $receiver): string {
    return $receiver->combine('dynamic-first', 'dynamic-second');
}

var_dump(invokeDynamicMethod($fixture));

class MethodArgumentPropertyHolder {
    public string $first = 'property-first';
    public string $second = 'property-second';
}

$holder = new MethodArgumentPropertyHolder();
var_dump($fixture->combine($holder->first, $holder->second));

function invokeDynamicPropertyArguments(object $receiver, object $holder): string {
    return $receiver->combine($holder->first, $holder->second);
}

var_dump(invokeDynamicPropertyArguments($fixture, $holder));

class VariadicMethodArgumentFixture {
    public function collect(string ...$args): string {
        return isset($args[0]) ? $args[0] : 'missing';
    }

    public function invokeInternally(): string {
        return $this->collect('internal-variadic');
    }
}

$variadic = new VariadicMethodArgumentFixture();
var_dump($variadic->invokeInternally());
