<?php
// oracle-probe: id=oracle-builtin-contract-function-unpack-efd1b2ff2a area=builtin_contract kind=function symbol=unpack source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-unpack-efd1b2ff2a failure_category=builtin_contract
try {
    $result = \unpack();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
