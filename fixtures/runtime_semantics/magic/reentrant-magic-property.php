<?php
#[AllowDynamicProperties]
class ReentrantMagicProperty {
    public int $sets = 0;
    public int $issets = 0;
    public int $unsets = 0;

    public function __set(string $name, mixed $value): void {
        $this->sets++;
        $this->$name = $value;
    }

    public function __isset(string $name): bool {
        $this->issets++;
        return isset($this->$name);
    }

    public function __unset(string $name): void {
        $this->unsets++;
        unset($this->$name);
    }
}

$object = new ReentrantMagicProperty();
$object->dynamic = "value";
echo $object->dynamic, "\n";
var_dump(isset($object->missing));
unset($object->missing);
echo $object->sets, ":", $object->issets, ":", $object->unsets, "\n";
