--TEST--
SPL generated SplFixedArray MVP covers sizing, ArrayAccess, toArray, and foreach
--FILE--
<?php
$fixed = new SplFixedArray(3);
$fixed[1] = 'middle';
echo $fixed->getSize(), '|', count($fixed), '|', $fixed[1], "\n";
foreach ($fixed as $key => $value) {
    if ($value === null) {
        echo "$key=null\n";
    } else {
        echo "$key=$value\n";
    }
}
$array = $fixed->toArray();
echo $array[1], "\n";
$fixed->setSize(2);
echo $fixed->getSize(), "\n";
$fixed->offsetUnset(1);
echo $fixed->offsetExists(1) ? "has-1\n" : "missing-1\n";
?>
--EXPECT--
3|3|middle
0=null
1=middle
2=null
middle
2
missing-1
