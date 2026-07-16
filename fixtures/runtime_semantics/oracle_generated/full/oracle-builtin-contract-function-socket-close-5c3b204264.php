<?php
// oracle-probe: id=oracle-builtin-contract-function-socket-close-5c3b204264 area=builtin_contract kind=function symbol=socket_close source=ext/sockets/sockets.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-socket-close-5c3b204264 failure_category=builtin_contract requires_ref_extension=sockets
$name = "socket_close";
echo function_exists($name) ? "available\n" : "missing\n";
