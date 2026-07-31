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
class MagicDim {
    private ArrayObject $bag;
    public function __construct() { $this->bag = new ArrayObject(['x' => 1, 'drop' => 2]); }
    public function __get($name) { return $this->bag; }
    public function __set($name, $value): void { throw new Error("unexpected __set"); }
    public function __unset($name): void { throw new Error("unexpected __unset"); }
    public function state(): array { return [$this->bag['x'], isset($this->bag['drop'])]; }
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
function dyn_fetch($o, string $name) { return $o->$name; }
function dyn_isset($o, string $name) { return isset($o->$name); }
function dyn_empty($o, string $name) { return empty($o->$name); }
function dyn_dim_isset($o, string $name, $key) { return isset($o->$name[$key]); }
function dyn_dim_empty($o, string $name, $key) { return empty($o->$name[$key]); }
function dyn_dim_assign($o, string $name, $key, $value) { return $o->$name[$key] = $value; }
function dyn_dim_nested_assign($o, string $name, $outer, $inner, $value) { return $o->$name[$outer][$inner] = $value; }
function dyn_dim_append($o, string $name, $value) { return $o->$name[] = $value; }
function dyn_dim_unset($o, string $name, $key): void { unset($o->$name[$key]); }
function dyn_unset($o, string $name): void { unset($o->$name); }
$std = new stdClass();
var_dump(dyn_assign($std, 'a', 1));
var_dump($std->a);
var_dump(dyn_assign($std, 'a', 2));
var_dump(dyn_fetch($std, 'a'));
var_dump(dyn_isset($std, 'a'));
var_dump(dyn_empty($std, 'a'));
$shared = ['present' => 4, 'zero' => 0, 'nested' => ['old' => 1]];
var_dump(dyn_assign($std, 'bag', $shared));
var_dump(dyn_dim_isset($std, 'bag', 'present'));
var_dump(dyn_dim_isset($std, 'bag', 'missing'));
var_dump(dyn_dim_empty($std, 'bag', 'present'));
var_dump(dyn_dim_empty($std, 'bag', 'zero'));
var_dump(dyn_dim_empty($std, 'bag', 'missing'));
var_dump(dyn_dim_assign($std, 'bag', 'present', 7));
var_dump($std->bag['present']);
var_dump($shared['present']);
var_dump(dyn_dim_nested_assign($std, 'bag', 'nested', 'new', 8));
var_dump($std->bag['nested']);
var_dump(dyn_dim_append($std, 'bag', 9));
var_dump($std->bag);
dyn_dim_unset($std, 'bag', 'zero');
var_dump(isset($std->bag['zero']));
$magic_dim = new MagicDim();
var_dump(dyn_dim_assign($magic_dim, 'bag', 'x', 5));
dyn_dim_unset($magic_dim, 'bag', 'drop');
var_dump($magic_dim->state());
dyn_unset($std, 'a');
var_dump(dyn_isset($std, 'a'));
$ao = new ArrayObject([], ArrayObject::ARRAY_AS_PROPS);
var_dump(dyn_assign($ao, 'k', 9));
var_dump($ao['k']);
$closure = function () {};
try {
    dyn_assign($closure, 'p', 1);
} catch (Error $e) {
    echo "Error: ", $e->getMessage(), "\n";
}
