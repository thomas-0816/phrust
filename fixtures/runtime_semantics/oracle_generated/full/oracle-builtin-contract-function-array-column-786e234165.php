<?php
// oracle-probe: id=oracle-builtin-contract-function-array-column-786e234165 area=builtin_contract kind=function symbol=array_column source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-column-786e234165 failure_category=builtin_contract
try {
    $result = \array_column();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
