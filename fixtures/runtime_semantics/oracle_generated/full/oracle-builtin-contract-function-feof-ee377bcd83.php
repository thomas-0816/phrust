<?php
// oracle-probe: id=oracle-builtin-contract-function-feof-ee377bcd83 area=builtin_contract kind=function symbol=feof source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-feof-ee377bcd83 failure_category=builtin_contract
try {
    $result = \feof();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
