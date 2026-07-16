<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-send-aa739fda09 area=builtin_contract kind=function symbol=socket_send source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-send-aa739fda09 failure_category=builtin_contract requires_ref_extension=sockets
$name = "socket_send";
echo function_exists($name) ? "available\n" : "missing\n";
