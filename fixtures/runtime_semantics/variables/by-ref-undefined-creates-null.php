<?php
// runtime-semantics: category=variables expect=pass
function write_ref(&$value) {
    var_dump($value);
    $value = 5;
}
write_ref($missing);
echo $missing, "\n";
