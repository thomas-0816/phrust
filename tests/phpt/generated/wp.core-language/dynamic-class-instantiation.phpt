--TEST--
Generated wp.core-language: dynamic class instantiation
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: application factories commonly instantiate classes from strings
oracle: Reference PHP 8.5.7
--FILE--
<?php
class WpWave3DynamicClass {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function label() {
        return "class:" . $this->name;
    }
}

$class = "WpWave3DynamicClass";
$obj = new $class("boot");
echo $obj->label(), "\n";
?>
--EXPECT--
class:boot
