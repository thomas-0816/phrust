<?php
// oracle-probe: id=oracle-builtin-contract-function-ob-get-contents-0f8b756486 area=builtin_contract kind=function symbol=ob_get_contents source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ob-get-contents-0f8b756486 failure_category=builtin_contract
try {
    $result = \ob_get_contents(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
