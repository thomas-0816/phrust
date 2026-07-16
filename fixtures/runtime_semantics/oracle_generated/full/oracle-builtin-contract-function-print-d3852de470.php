<?php
// oracle-probe: id=oracle-builtin-contract-function-print-d3852de470 area=builtin_contract kind=function symbol=print source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-print-d3852de470 failure_category=builtin_contract
$name = "print";
echo function_exists($name) ? "available\n" : "missing\n";
