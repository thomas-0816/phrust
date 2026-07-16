<?php
// oracle-probe: id=oracle-builtin-contract-function-session-register-shutdown-3a5606dc01 area=builtin_contract kind=function symbol=session_register_shutdown source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-register-shutdown-3a5606dc01 failure_category=builtin_contract requires_ref_extension=session
$name = "session_register_shutdown";
echo function_exists($name) ? "available\n" : "missing\n";
