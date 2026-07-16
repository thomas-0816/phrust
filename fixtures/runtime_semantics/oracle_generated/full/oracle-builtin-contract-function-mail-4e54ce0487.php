<?php
// oracle-probe: id=oracle-builtin-contract-function-mail-4e54ce0487 area=builtin_contract kind=function symbol=mail source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-mail-4e54ce0487 failure_category=builtin_contract
try {
    $result = \mail();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
