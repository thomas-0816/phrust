<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-read-55173178dc area=builtin_contract kind=function symbol=socket_read source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-read-55173178dc failure_category=builtin_contract requires_ref_extension=sockets
try {
    $result = \socket_read();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
