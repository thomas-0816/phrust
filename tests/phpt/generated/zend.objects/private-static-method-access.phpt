--TEST--
Generated zend.objects: accessible private/protected static method calls
--DESCRIPTION--
module: objects.classes
generated timestamp: 20260627T000000Z
generator version: phpt-objects-classes-v1
reason: a class calling its own private static method dispatches normally instead of routing to __callStatic; only inaccessible private/protected static methods take the magic/error path (tests/Zend/tests/private_007.phpt)
--FILE--
<?php
class Bar {
    public static function pub() { Bar::priv(); }
    private static function priv() { echo "Bar::priv()\n"; }
}
class Foo extends Bar {
    public static function priv() { echo "Foo::priv()\n"; }
}
Foo::pub();
Foo::priv();
echo "Done\n";
?>
--EXPECT--
Bar::priv()
Foo::priv()
Done
