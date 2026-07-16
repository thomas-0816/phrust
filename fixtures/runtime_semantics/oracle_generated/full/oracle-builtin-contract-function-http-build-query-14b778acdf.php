<?php
// oracle-probe: id=oracle-builtin-contract-function-http-build-query-14b778acdf area=builtin_contract kind=function symbol=http_build_query source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-http-build-query-14b778acdf failure_category=builtin_contract
try {
    $result = \http_build_query();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
