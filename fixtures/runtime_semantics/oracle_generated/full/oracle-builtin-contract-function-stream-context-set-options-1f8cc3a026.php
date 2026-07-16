<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-context-set-options-1f8cc3a026 area=builtin_contract kind=function symbol=stream_context_set_options source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-context-set-options-1f8cc3a026 failure_category=builtin_contract
try {
    $result = \stream_context_set_options();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
