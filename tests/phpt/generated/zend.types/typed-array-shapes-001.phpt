--TEST--
Generated zend.types: typed array shapes (T[], T[][]) with implicit coercion
--DESCRIPTION--
module: zend.types
generated timestamp: 20260707T213000Z
generator version: phpt-zend.types-v1
reason: Lock in parameter type checking, promoted constructor properties, and property assignment coercion for typed array shapes (int[], string[], float[][]) in weak mode
--FILE--
<?php

class Point {
    public function __construct(public int $x, public int $y) {}
}

function sum_int(int[] $a): int {
    $s = 0;
    foreach ($a as $v) { $s += $v; }
    return $s;
}

function join_str(string[] $a): string {
    return implode(',', $a);
}

function flatten(float[][] $a): float {
    $s = 0.0;
    foreach ($a as $row) { foreach ($row as $v) { $s += $v; } }
    return $s;
}

function count_points(Point[] $points): int {
    return count($points);
}

function handle(int[]|string $a): string {
    if (is_string($a)) { return "str:$a"; }
    return "arr:" . count($a);
}

class Container {
    public function __construct(
        public int[] $nums,
        public string[] $strs,
    ) {}
}

echo "=== Parameter coercion ===\n";
echo sum_int(['1', '2', '3']) . "\n";
echo join_str([1, 2, 3]) . "\n";
echo flatten([['1.5', '2.5'], ['3.5', '4.5']]) . "\n";
echo count_points([new Point(1, 2), new Point(3, 4)]) . "\n";
echo handle(['1', '2', '3']) . "\n";
echo handle('hello') . "\n";

echo "=== Promoted constructor ===\n";
$c = new Container(['1', '2'], [1, 2, 3]);
echo "nums: " . implode(',', $c->nums) . "\n";
echo "strs: " . implode(',', $c->strs) . "\n";

echo "=== Property assignment coercion ===\n";
$c->nums = ['10', '20', '30'];
$c->strs = [4, 5, 6];
echo "nums: " . implode(',', $c->nums) . "\n";
echo "strs: " . implode(',', $c->strs) . "\n";

echo "=== Type error on non-coercible values ===\n";
try {
    sum_int(["hello"]);
} catch (TypeError $e) {
    echo "int[] error: " . $e->getMessage() . "\n";
}

try {
    $c->nums = [new Point(0, 0)];
} catch (TypeError $e) {
    echo "property error: " . $e->getMessage() . "\n";
}

echo "=== Nullable typed array ===\n";
function nullable_int(?int[] $a): int {
    if ($a === null) { return -1; }
    return count($a);
}
echo nullable_int(['1', '2', '3']) . "\n";
echo nullable_int(null) . "\n";

echo "DONE\n";
--EXPECTF--
=== Parameter coercion ===
6
1,2,3
12
2
arr:3
str:hello
=== Promoted constructor ===
nums: 1,2
strs: 1,2,3
=== Property assignment coercion ===
nums: 10,20,30
strs: 4,5,6
=== Type error on non-coercible values ===
int[] error: sum_int(): Argument #1 ($a) must be of type int[], array given, called in %s on line %d
property error: property Container::$nums got %s, expected int[]
=== Nullable typed array ===
3
-1
DONE
