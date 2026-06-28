--TEST--
SPL generated SplObjectStorage MVP uses object identity for offset methods and iteration
--FILE--
<?php
$a = new stdClass();
$b = new stdClass();
$storage = new SplObjectStorage();
$storage->offsetSet($a, 'alpha');
$storage->offsetSet($b, 'beta');
echo $storage->offsetExists($a) ? "has-a\n" : "missing-a\n";
echo $storage->offsetGet($b), "\n";
echo count($storage), "\n";
foreach ($storage as $key => $value) {
    echo "$key:", get_class($value), "\n";
}
$storage->offsetUnset($a);
echo count($storage), "\n";
?>
--EXPECT--
has-a
beta
2
0:stdClass
1:stdClass
1
