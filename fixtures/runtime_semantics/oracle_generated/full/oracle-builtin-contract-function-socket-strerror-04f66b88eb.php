<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-strerror-04f66b88eb area=builtin_contract kind=function symbol=socket_strerror source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-strerror-04f66b88eb failure_category=builtin_contract requires_ref_extension=sockets
try {
    $result = \socket_strerror();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
