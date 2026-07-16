<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-socket-server-4a89b68993 area=builtin_contract kind=function symbol=stream_socket_server source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-socket-server-4a89b68993 failure_category=builtin_contract
$name = "stream_socket_server";
echo function_exists($name) ? "available\n" : "missing\n";
