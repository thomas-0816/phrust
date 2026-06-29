--TEST--
Generated zend.objects: public static methods with self and parent
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-static-v1
reason: public static method baseline
--FILE--
<?php
class Base {
    public static function name() {
        return "Base";
    }
}

class Child extends Base {
    public static function name() {
        return "Child";
    }

    public static function labels() {
        return self::name() . "|" . parent::name();
    }
}

echo Child::labels(), "\n";
?>
--EXPECT--
Child|Base
