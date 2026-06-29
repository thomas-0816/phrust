--TEST--
Generated zend.objects: constructor initializes public property
--DESCRIPTION--
module: zend.objects
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
