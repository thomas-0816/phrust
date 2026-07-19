<?php
class ReferenceBox
{
    public int $value = 0;
}

$box = new ReferenceBox();
$reference =& $box;
$reference->value = 7;
echo $box->value, "\n";

class NestedReferenceBox
{
    private $values;

    public function __construct($values = null)
    {
        $this->values = $values ?? [
            'a' => ['value' => 1],
            'b' => ['value' => 2],
            'c' => ['value' => 3],
        ];
    }

    public function loadFirst(): void
    {
        $values =& $this->values;
        $values['a']['loaded'] = true;
    }

    public function keys(): array
    {
        return array_keys($this->values);
    }
}

$nested = new NestedReferenceBox();
$nested->loadFirst();
echo implode(',', $nested->keys()), "\n";
