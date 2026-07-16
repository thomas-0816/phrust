<?php
// oracle-probe: id=oracle-builtin-contract-function-gethostbyname-93d6ff3495 area=builtin_contract kind=function symbol=gethostbyname source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-gethostbyname-93d6ff3495 failure_category=builtin_contract
try {
    $result = \gethostbyname();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
