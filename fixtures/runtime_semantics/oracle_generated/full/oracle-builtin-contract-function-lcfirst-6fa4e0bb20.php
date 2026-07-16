<?php
// oracle-probe: id=oracle-builtin-contract-function-lcfirst-6fa4e0bb20 area=builtin_contract kind=function symbol=lcfirst source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-lcfirst-6fa4e0bb20 failure_category=builtin_contract
try {
    $result = \lcfirst();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
