<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-is-local-7ba24b4737 area=builtin_contract kind=function symbol=stream_is_local source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-is-local-7ba24b4737 failure_category=builtin_contract
try {
    $result = \stream_is_local();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
