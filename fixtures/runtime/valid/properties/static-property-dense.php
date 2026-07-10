<?php
// Regression: static-property assignment and isset/empty probes must execute
// densely (previously dropped whole method bodies to the rich interpreter).
// Covers literal/self/static class names, typed statics, undeclared-property
// and visibility Errors, inherited statics, replaced-value destructors, and
// assignment-expression results.
class Counter {
    public static int $count = 0;
    private static array $log = [];
    protected static $free = null;
    public static function bump(int $by): int {
        return self::$count = self::$count + $by;
    }
    public static function lateBump(): int {
        return static::$count = static::$count + 10;
    }
    public static function record(string $entry): void {
        self::$log[] = $entry;
    }
    public static function snapshot(): array {
        return self::$log;
    }
    public static function probes(): array {
        return [isset(self::$count), empty(self::$count), isset(static::$free), empty(self::$free)];
    }
}
class SubCounter extends Counter {}

var_dump(Counter::bump(2));
var_dump(SubCounter::lateBump());
Counter::record('a');
Counter::record('b');
print_r(Counter::snapshot());
print_r(Counter::probes());
var_dump(Counter::$count);

function typed_fail() { Counter::$count = 'nope'; }
try { typed_fail(); } catch (TypeError $e) { echo "TypeError: ", $e->getMessage(), "\n"; }

function undeclared() { return isset(Counter::$missing); }
try { var_dump(undeclared()); } catch (Error $e) { echo "Error: ", $e->getMessage(), "\n"; }

function assign_undeclared() { Counter::$missing = 1; }
try { assign_undeclared(); } catch (Error $e) { echo "Error: ", $e->getMessage(), "\n"; }

class Dtor { public function __destruct() { echo "dtor\n"; } }
class Holder { public static $slot = null; }
function replace_static() { Holder::$slot = new Dtor(); Holder::$slot = 'replaced'; }
replace_static();
var_dump(Holder::$slot);
