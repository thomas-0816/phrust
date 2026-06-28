--TEST--
SPL generated interfaces expose reflection and implementation metadata
--FILE--
<?php
$interfaces = [
    'Countable',
    'Iterator',
    'IteratorAggregate',
    'ArrayAccess',
    'SeekableIterator',
    'RecursiveIterator',
];
foreach ($interfaces as $name) {
    echo interface_exists($name) ? strtolower($name) . "\n" : "missing:$name\n";
}

$it = new RecursiveArrayIterator(['x' => 1]);
echo ($it instanceof RecursiveIterator) ? "recursive\n" : "not-recursive\n";
echo ($it instanceof SeekableIterator) ? "seekable\n" : "not-seekable\n";
echo (new ArrayIterator([]) instanceof RecursiveIterator) ? "array-recursive\n" : "array-not-recursive\n";

$method = new ReflectionMethod('ArrayIterator', 'current');
echo $method->getName(), '|', $method->getNumberOfParameters(), '|', strtolower($method->getExtensionName()), "\n";
?>
--EXPECT--
countable
iterator
iteratoraggregate
arrayaccess
seekableiterator
recursiveiterator
recursive
seekable
array-not-recursive
current|0|spl
