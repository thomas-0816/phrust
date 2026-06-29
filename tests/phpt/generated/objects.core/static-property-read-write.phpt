--TEST--
Generated objects.core: public static property read and write
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-static-v1
reason: static property read/write baseline
--FILE--
<?php
class Counter {
    public static $value = 1;

    public static function inc() {
        self::$value = self::$value + 1;
        return self::$value;
    }
}

echo Counter::$value, "\n";
Counter::$value = 4;
echo Counter::$value, "\n";
echo Counter::inc(), "\n";
?>
--EXPECT--
1
4
5
