<?php
class GlobalMethodTarget {
    public function add_rule(): void {
        echo "ok\n";
    }
}

require __DIR__ . '/_data/global-object-method-after-include-child.php';

var_dump(read_global_method_target());
$GLOBALS['global_method_target'] = new GlobalMethodTarget();
invoke_global_method_target();
