<?php
// runtime-semantics: category=statics expect=pass
function next_static() {
    static $value = 0;
    $value++;
    return $value;
}
echo next_static(), ":", next_static(), "\n";
