<?php
// oracle-probe: id=oracle-builtin-contract-function-print-c9798796da area=builtin_contract kind=function symbol=print source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-print-c9798796da failure_category=builtin_contract
try {
    $result = \print(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
