<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-bind-d9a8b2f354 area=builtin_contract kind=function symbol=socket_bind source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-bind-d9a8b2f354 failure_category=builtin_contract requires_ref_extension=sockets
$name = "socket_bind";
echo function_exists($name) ? "available\n" : "missing\n";
