<?php
// Root membership must follow object properties, references, and array
// element references without delaying or duplicating observable destructors.
class RootTracked {
    public function __construct(public int $id) {}
    public function __destruct() { echo "drop:", $this->id, "\n"; }
}
class RootBox { public $value = null; }

$box = new RootBox();
$box->value = new RootTracked(1);
$box->value = 7;
echo "property\n";

$slot =& $box->value;
$slot = new RootTracked(2);
$slot = 9;
echo "property-ref\n";

$array = [];
$array['value'] = new RootTracked(3);
$arraySlot =& $array['value'];
$arraySlot = null;
echo "array-ref\n";
