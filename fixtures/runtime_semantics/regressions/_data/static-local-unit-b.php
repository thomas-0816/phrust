<?php

function static_local_unit_b() {
    static $state = 41;
    return ++$state;
}
