<?php

function static_local_unit_a() {
    static $state = array('a');
    return $state[0];
}
