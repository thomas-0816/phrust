<?php
class GeneratedMagicCloneFixture {
    public int $value = 1;

    public function __clone() {
        $this->value = 7;
        echo "cloned\n";
    }
}

function generated_magic_clone(object $object) {
    return clone $object;
}

$original = new GeneratedMagicCloneFixture();
$copy = generated_magic_clone($original);
echo $original->value, '|', $copy->value, "\n";
