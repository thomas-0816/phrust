<?php
class NumericKeySortHolder {
    public array $callbacks = [20 => "last", 1 => "first", 10 => "middle"];

    public function sortCallbacks(): void {
        ksort($this->callbacks, SORT_NUMERIC);
    }
}

$holder = new NumericKeySortHolder();
$holder->sortCallbacks();
foreach ($holder->callbacks as $priority => $value) {
    echo $priority, ":", $value, "\n";
}
