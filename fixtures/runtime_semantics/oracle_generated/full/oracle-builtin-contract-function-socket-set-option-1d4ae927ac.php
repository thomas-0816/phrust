<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-set-option-1d4ae927ac area=builtin_contract kind=function symbol=socket_set_option source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-set-option-1d4ae927ac failure_category=builtin_contract requires_ref_extension=sockets
try {
    $result = \socket_set_option();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
