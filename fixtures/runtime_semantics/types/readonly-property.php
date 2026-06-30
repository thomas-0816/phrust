<?php
// runtime-semantics: category=types expect=pass
class Box {
    public readonly int $value;

    public function __construct(int $value) {
        $this->value = $value;
    }
}

$box = new Box(1);
echo $box->value, "\n";
