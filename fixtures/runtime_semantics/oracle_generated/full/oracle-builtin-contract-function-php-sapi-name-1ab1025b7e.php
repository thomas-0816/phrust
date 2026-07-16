<?php
// oracle-probe: id=oracle-builtin-contract-function-php-sapi-name-1ab1025b7e area=builtin_contract kind=function symbol=php_sapi_name source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-php-sapi-name-1ab1025b7e failure_category=builtin_contract
try {
    $result = \php_sapi_name(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
