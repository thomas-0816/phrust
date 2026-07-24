<?php
class ByRefDefaultForwardTarget {
    public function get(&$value = null) {
        $value = 7;
        return $value;
    }
}

function by_ref_default_forward($target, &$value = null) {
    $result = $target->get($value);
    echo $result, '|', $value, "\n";
}

$target = new ByRefDefaultForwardTarget();
by_ref_default_forward($target);
