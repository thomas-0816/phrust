<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-create-d532b0b1e9 area=builtin_contract kind=function symbol=socket_create source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-create-d532b0b1e9 failure_category=builtin_contract requires_ref_extension=sockets
try {
    $result = \socket_create();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
