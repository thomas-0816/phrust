<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-getpeername-cc96c372d0 area=builtin_contract kind=function symbol=socket_getpeername source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-getpeername-cc96c372d0 failure_category=builtin_contract requires_ref_extension=sockets
try {
    $result = \socket_getpeername();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
