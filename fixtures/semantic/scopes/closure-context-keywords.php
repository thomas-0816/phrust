<?php
// Reference-confirmed (PHP 8.5.7): self/static/parent inside closure and
// arrow-function bodies are deferred to invocation time and must not produce
// class-context diagnostics, in any lexical scope.
$a = function () { return self::class; };
$b = fn () => static::class;
$c = static function () { return parent::class; };

class Plain
{
    public function deferred(): \Closure
    {
        return function () { return parent::class; };
    }
}
