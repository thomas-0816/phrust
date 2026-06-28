--TEST--
Generated objects.core: $this inside method accesses object state
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-basics-v1
reason: Prompt 14.3 $this method state baseline
--FILE--
<?php
class Counter {
    public $value = 0;

    public function inc() {
        $this->value = $this->value + 1;
        return $this->value;
    }
}

$counter = new Counter();
echo $counter->inc(), "\n";
echo $counter->inc(), "\n";
?>
--EXPECT--
1
2
