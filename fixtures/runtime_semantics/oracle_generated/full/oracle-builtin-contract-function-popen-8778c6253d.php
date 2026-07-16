<?php
// oracle-probe: id=oracle-builtin-contract-function-popen-8778c6253d area=builtin_contract kind=function symbol=popen source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-popen-8778c6253d failure_category=builtin_contract
try {
    $result = \popen();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
