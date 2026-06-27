--TEST--
Generated zend.objects: print_r annotates property visibility
--DESCRIPTION--
module: objects.classes
generated timestamp: 20260627T000000Z
generator version: phpt-objects-classes-v1
reason: print_r labels protected properties as name:protected and private properties as name:Class:private (no quotes), matching PHP
--FILE--
<?php
class C {
    public $a = 1;
    protected $b = 2;
    private $c = 3;
}
print_r(new C());
?>
--EXPECT--
C Object
(
    [a] => 1
    [b:protected] => 2
    [c:C:private] => 3
)
