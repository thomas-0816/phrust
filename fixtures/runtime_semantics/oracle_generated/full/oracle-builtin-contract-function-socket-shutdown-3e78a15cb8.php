<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-shutdown-3e78a15cb8 area=builtin_contract kind=function symbol=socket_shutdown source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-shutdown-3e78a15cb8 failure_category=builtin_contract requires_ref_extension=sockets
$name = "socket_shutdown";
echo function_exists($name) ? "available\n" : "missing\n";
