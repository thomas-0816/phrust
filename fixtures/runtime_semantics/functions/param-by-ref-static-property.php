<?php
// runtime-semantics: category=functions expect=pass
class StaticRefBox {
    public static $value = 1;
}

function bump_static_ref(&$value) {
    $value++;
}

bump_static_ref(StaticRefBox::$value);
echo StaticRefBox::$value;
