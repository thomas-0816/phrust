--TEST--
SPL generated ArrayObject MVP covers ArrayAccess, Countable, foreach, and exchangeArray
--FILE--
<?php
$object = new ArrayObject(['a' => 1]);
$object['b'] = 2;
$object->append(3);
echo count($object), "\n";
echo $object->offsetExists('a') ? "has-a\n" : "missing-a\n";
foreach ($object as $key => $value) {
    echo "$key=$value\n";
}
$old = $object->exchangeArray(['z' => 9]);
echo $old['b'], "\n";
echo $object['z'], "\n";
$object->offsetUnset('z');
echo $object->offsetExists('z') ? "has-z\n" : "missing-z\n";
?>
--EXPECT--
3
has-a
a=1
b=2
0=3
2
9
missing-z
