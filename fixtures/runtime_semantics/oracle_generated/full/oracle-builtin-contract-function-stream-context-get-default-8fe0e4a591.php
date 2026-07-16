<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-context-get-default-8fe0e4a591 area=builtin_contract kind=function symbol=stream_context_get_default source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-context-get-default-8fe0e4a591 failure_category=builtin_contract
try {
    $result = \stream_context_get_default(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
