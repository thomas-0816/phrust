--TEST--
Generated zend.objects: public instance method call and return
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-basics-v1
reason: public method call baseline
--FILE--
<?php
class Greeter {
    public function greet($name) {
        return "Hello " . $name;
    }
}

echo (new Greeter())->greet("World"), "\n";
?>
--EXPECT--
Hello World
