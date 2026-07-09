<?php
// Reference-confirmed (PHP 8.5.7): closure and arrow-function bodies compile
// with self/static/parent in any lexical scope; the class scope is resolved
// when the closure is invoked or rebound, so none of these may produce a
// compile-time diagnostic.
$a = function () { return self::class; };
$b = fn () => static::class;
$c = static function () { return parent::class; };
class NoParent {
    public function m(): \Closure
    {
        return function () { return parent::class; };
    }
}
echo "compiled";
