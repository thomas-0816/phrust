--TEST--
Generated objects.core: public instance method call and return
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-basics-v1
reason: Prompt 14.3 public method call baseline
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
