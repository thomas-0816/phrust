<?php
// Regression: unset($obj->prop[$k]) must execute densely (it previously
// dropped whole method bodies to the rich interpreter). Covers string and
// int keys, nested dims, missing keys, and declared + dynamic properties.
class Bag {
    public array $items = ['a' => 1, 'b' => 2, 'c' => ['x' => 10, 'y' => 20]];
    public function drop(string $key): void {
        unset($this->items[$key]);
    }
    public function dropNested(string $outer, string $inner): void {
        unset($this->items[$outer][$inner]);
    }
}
$bag = new Bag();
$bag->drop('a');
$bag->drop('missing');
$bag->dropNested('c', 'y');
print_r($bag->items);
#[AllowDynamicProperties]
class OpenBag {}
$open = new OpenBag();
$open->dynamic = [1, 2, 3];
unset($open->dynamic[1]);
print_r($open->dynamic);
