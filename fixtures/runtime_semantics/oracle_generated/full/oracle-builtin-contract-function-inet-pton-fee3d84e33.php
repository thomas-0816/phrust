<?php
// oracle-probe: id=oracle-builtin-contract-function-inet-pton-fee3d84e33 area=builtin_contract kind=function symbol=inet_pton source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-inet-pton-fee3d84e33 failure_category=builtin_contract
try {
    $result = \inet_pton();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
