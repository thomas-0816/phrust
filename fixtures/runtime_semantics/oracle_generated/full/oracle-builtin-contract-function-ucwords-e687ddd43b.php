<?php
// oracle-probe: id=oracle-builtin-contract-function-ucwords-e687ddd43b area=builtin_contract kind=function symbol=ucwords source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ucwords-e687ddd43b failure_category=builtin_contract
try {
    $result = \ucwords();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
