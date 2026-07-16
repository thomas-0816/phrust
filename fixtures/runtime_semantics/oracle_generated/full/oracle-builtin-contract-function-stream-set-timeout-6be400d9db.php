<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-set-timeout-6be400d9db area=builtin_contract kind=function symbol=stream_set_timeout source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-set-timeout-6be400d9db failure_category=builtin_contract
try {
    $result = \stream_set_timeout();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
