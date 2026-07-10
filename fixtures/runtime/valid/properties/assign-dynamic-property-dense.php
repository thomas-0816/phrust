<?php
// Regression: $obj->$name = $v must execute densely (previously dropped whole
// method bodies to the rich interpreter). Covers declared and typed
// properties, __set magic, stdClass, ARRAY_AS_PROPS containers, a Closure
// receiver raise, and assignment-expression results.
class Typed {
    public int $count = 0;
    private string $secret = 's';
    public array $bag = [];
    public function set(string $name, $value) { return $this->$name = $value; }
}
class Magic {
    private array $data = [];
    public function __set($name, $value) { $this->data[$name] = $value; }
    public function dump(): array { return $this->data; }
    public function set(string $name, $value) { return $this->$name = $value; }
}
$t = new Typed();
var_dump($t->set('count', 3));
var_dump($t->count);
try {
    $t->set('count', 'not-an-int');
} catch (TypeError $e) {
    echo "TypeError: ", $e->getMessage(), "\n";
}
$m = new Magic();
var_dump($m->set('hidden', 'x'));
print_r($m->dump());

function dyn_assign($o, string $name, $value) { return $o->$name = $value; }
$std = new stdClass();
var_dump(dyn_assign($std, 'a', 1));
var_dump($std->a);
$ao = new ArrayObject([], ArrayObject::ARRAY_AS_PROPS);
var_dump(dyn_assign($ao, 'k', 9));
var_dump($ao['k']);
$closure = function () {};
try {
    dyn_assign($closure, 'p', 1);
} catch (Error $e) {
    echo "Error: ", $e->getMessage(), "\n";
}
