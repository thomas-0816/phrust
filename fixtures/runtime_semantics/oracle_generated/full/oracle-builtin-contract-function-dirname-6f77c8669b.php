<?php
// oracle-probe: id=oracle-builtin-contract-function-dirname-6f77c8669b area=builtin_contract kind=function symbol=dirname source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-dirname-6f77c8669b failure_category=builtin_contract
try {
    $result = \dirname();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
