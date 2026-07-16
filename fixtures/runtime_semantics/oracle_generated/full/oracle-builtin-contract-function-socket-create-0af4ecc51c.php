<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-create-0af4ecc51c area=builtin_contract kind=function symbol=socket_create source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-create-0af4ecc51c failure_category=builtin_contract requires_ref_extension=sockets
$name = "socket_create";
echo function_exists($name) ? "available\n" : "missing\n";
