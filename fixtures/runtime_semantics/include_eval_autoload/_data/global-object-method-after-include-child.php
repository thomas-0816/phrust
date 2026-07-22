<?php
function read_global_method_target() {
    global $global_method_target;
    return $global_method_target;
}

function invoke_global_method_target(): void {
    global $global_method_target;
    $global_method_target->add_rule();
}
