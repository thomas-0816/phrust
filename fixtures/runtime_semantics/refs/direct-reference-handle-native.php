<?php
// runtime-semantics: category=refs expect=pass php_ref_required=1
function native_reference_handle(string &$value, string $replacement): string {
    $value = $replacement;
    if (!isset($value)) {
        return "unset";
    }
    return empty($value) ? "empty" : $value;
}

$value = "old";
$alias =& $value;
var_dump(native_reference_handle($value, "replacement"));
var_dump($value, $alias);

var_dump(native_reference_handle($value, ""));
var_dump($value, $alias);
