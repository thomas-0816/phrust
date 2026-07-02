<?php
// runtime-semantics: expect=pass
namespace Demo\Calls;

function suffix($value) {
    return $value . "N";
}

$callable = __NAMESPACE__ . "\\suffix";
echo $callable("A");
