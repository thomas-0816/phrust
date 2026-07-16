<?php
// oracle-probe: id=oracle-builtin-contract-function-session-destroy-d5f6863929 area=builtin_contract kind=function symbol=session_destroy source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-destroy-d5f6863929 failure_category=builtin_contract requires_ref_extension=session
$name = "session_destroy";
echo function_exists($name) ? "available\n" : "missing\n";
