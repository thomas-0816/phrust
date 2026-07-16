<?php
// oracle-probe: id=oracle-builtin-contract-function-array-intersect-ukey-120c634b63 area=builtin_contract kind=function symbol=array_intersect_ukey source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-intersect-ukey-120c634b63 failure_category=builtin_contract
try {
    $result = \array_intersect_ukey();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
