<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-clear-error-47e7d17f82 area=builtin_contract kind=function symbol=socket_clear_error source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-clear-error-47e7d17f82 failure_category=builtin_contract requires_ref_extension=sockets
try {
    $result = \socket_clear_error(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
