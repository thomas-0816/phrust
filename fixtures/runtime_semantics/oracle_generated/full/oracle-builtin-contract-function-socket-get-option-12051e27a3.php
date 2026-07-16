<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-get-option-12051e27a3 area=builtin_contract kind=function symbol=socket_get_option source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-get-option-12051e27a3 failure_category=builtin_contract requires_ref_extension=sockets
$name = "socket_get_option";
echo function_exists($name) ? "available\n" : "missing\n";
