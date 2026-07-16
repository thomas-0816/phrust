<?php
error_reporting(E_ALL);
$array = ['' => 7];
var_dump(isset($array[null]));
var_dump(empty($array[null]));
echo $array[null], "\n";
$array[null] = 9;
$reference =& $array[null];
unset($array[null]);
var_dump($array);

class NullOffsetBox {
    public array $values = ['' => 11];

    public function check(mixed $key): void {
        var_dump(isset($this->values[$key]));
    }
}

(new NullOffsetBox())->check(null);
