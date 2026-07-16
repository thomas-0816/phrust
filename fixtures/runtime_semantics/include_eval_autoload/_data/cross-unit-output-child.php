<?php

class CrossUnitOutputChild extends CrossUnitOutputBase {
    public function emit($value) {
        echo '<', $value, '>';
    }
}
