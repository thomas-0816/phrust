--TEST--
SPL generated iterator MVP classes iterate deterministic array entries
--FILE--
<?php
$it = new ArrayIterator(['a' => 10, 'b' => 20]);
echo $it->key(), '=', $it->current(), "\n";
$it->next();
echo $it->key(), '=', $it->current(), "\n";
$it->rewind();
echo $it->valid() ? "valid\n" : "invalid\n";
foreach ($it as $key => $value) {
    echo "ai:$key=$value\n";
}

$wrapped = new IteratorIterator($it);
foreach ($wrapped as $key => $value) {
    echo "ii:$key=$value\n";
}

foreach (new LimitIterator(new ArrayIterator([10, 20, 30]), 1, 1) as $key => $value) {
    echo "limit:$key=$value\n";
}

$empty = new EmptyIterator();
echo $empty->valid() ? "empty-valid\n" : "empty-invalid\n";

$append = new AppendIterator();
$append->append(new ArrayIterator(['x' => 7]));
$append->append(new ArrayIterator(['y' => 8]));
foreach ($append as $key => $value) {
    echo "append:$key=$value\n";
}

$recursive = new RecursiveArrayIterator(['r' => 9]);
echo ($recursive instanceof RecursiveIterator) ? "recursive\n" : "not-recursive\n";
?>
--EXPECT--
a=10
b=20
valid
ai:a=10
ai:b=20
ii:a=10
ii:b=20
limit:1=20
empty-invalid
append:x=7
append:y=8
recursive
