<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-wrapper-register-49909d9a27 area=builtin_contract kind=function symbol=stream_wrapper_register source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-wrapper-register-49909d9a27 failure_category=builtin_contract
try {
    $result = \stream_wrapper_register();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
