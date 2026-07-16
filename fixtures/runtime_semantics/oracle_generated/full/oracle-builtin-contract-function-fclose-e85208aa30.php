<?php
// oracle-probe: id=oracle-builtin-contract-function-fclose-e85208aa30 area=builtin_contract kind=function symbol=fclose source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fclose-e85208aa30 failure_category=builtin_contract
try {
    $result = \fclose();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
