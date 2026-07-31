<?php
class PerfLinkedReferenceBox {
    public $values = [[1]];
}

function perf_linked_reference_lvalue_wrapper($box) {
    perf_linked_reference_increment($box->values[0][0]);
    return $box->values[0][0];
}

require __DIR__ . '/linked_reference_lvalue_publication_target.php';

$box = new PerfLinkedReferenceBox();
echo perf_linked_reference_lvalue_wrapper($box), '|', $box->values[0][0], "\n";
