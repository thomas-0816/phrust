<?php

class CrossUnitOutputBase {
    public function run($values) {
        foreach ($values as $value) {
            $this->emit($value);
        }
        return count($values);
    }

    public function emit($value) {
        echo '<base:', $value, '>';
    }
}
