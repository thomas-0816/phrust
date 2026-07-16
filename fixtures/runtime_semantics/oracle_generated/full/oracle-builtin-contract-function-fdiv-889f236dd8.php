<?php
// oracle-probe: id=oracle-builtin-contract-function-fdiv-889f236dd8 area=builtin_contract kind=function symbol=fdiv source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fdiv-889f236dd8 failure_category=builtin_contract
try {
    $result = \fdiv();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
