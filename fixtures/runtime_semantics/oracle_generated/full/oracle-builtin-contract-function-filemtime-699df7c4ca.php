<?php
// oracle-probe: id=oracle-builtin-contract-function-filemtime-699df7c4ca area=builtin_contract kind=function symbol=filemtime source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-filemtime-699df7c4ca failure_category=builtin_contract
try {
    $result = \filemtime();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
