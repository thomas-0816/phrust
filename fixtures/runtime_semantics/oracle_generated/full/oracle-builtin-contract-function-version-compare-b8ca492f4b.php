<?php
// oracle-probe: id=oracle-builtin-contract-function-version-compare-b8ca492f4b area=builtin_contract kind=function symbol=version_compare source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-version-compare-b8ca492f4b failure_category=builtin_contract
try {
    $result = \version_compare();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
