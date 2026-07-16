<?php
// oracle-probe: id=oracle-builtin-contract-function-session-status-2defa26eee area=builtin_contract kind=function symbol=session_status source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-status-2defa26eee failure_category=builtin_contract requires_ref_extension=session
$name = "session_status";
echo function_exists($name) ? "available\n" : "missing\n";
