<?php
// runtime-semantics: category=statics expect=pass
function unset_static_alias() {
    static $value = 0;
    $value++;
    echo $value, ":";
    unset($value);
    echo isset($value) ? "set" : "unset", "\n";
}
unset_static_alias();
unset_static_alias();
