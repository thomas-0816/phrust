<?php
// Regression: unset($obj->prop) must execute densely, honor typed-property
// reset semantics, __unset magic, visibility raises caught by the caller,
// and route ARRAY_AS_PROPS containers through offsetUnset (previously
// ignored on both interpreter paths).
class Guard {
    public int $typed = 5;
    private $hidden = 'h';
    public $plain = 'p';
    public function dropTyped(): void { unset($this->typed); }
    public function dropPlain(): void { unset($this->plain); }
}
class Magic {
    private array $data = ['m' => 1];
    public function __unset($name) { echo "__unset($name)\n"; unset($this->data[$name]); }
    public function dump(): array { return $this->data; }
}
$g = new Guard();
$g->dropTyped();
$g->dropPlain();
var_dump(isset($g->typed), isset($g->plain));
function drop_hidden($o) { unset($o->hidden); }
try { drop_hidden($g); } catch (Error $e) { echo "Error: ", $e->getMessage(), "\n"; }
$m = new Magic();
function drop_magic($o) { unset($o->m); }
drop_magic($m);
print_r($m->dump());
$ao = new ArrayObject(['k' => 1, 'j' => 2], ArrayObject::ARRAY_AS_PROPS);
function drop_spl($o) { unset($o->k); }
drop_spl($ao);
unset($ao->j);
var_dump($ao->getArrayCopy());
