<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-copy-to-stream-4bd028e5c2 area=builtin_contract kind=function symbol=stream_copy_to_stream source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-copy-to-stream-4bd028e5c2 failure_category=builtin_contract
try {
    $result = \stream_copy_to_stream();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
