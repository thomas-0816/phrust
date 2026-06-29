--TEST--
Generated objects.core: constructor initializes public property
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-basics-v1
reason: constructor/property baseline
--FILE--
<?php
class C {
    public $value;

    public function __construct($value) {
        $this->value = $value;
    }
}

$c = new C(7);
echo $c->value, "\n";
?>
--EXPECT--
7
