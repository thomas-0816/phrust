<?php
// oracle-probe: id=oracle-builtin-contract-function-session-save-path-a59eef8e8b area=builtin_contract kind=function symbol=session_save_path source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-save-path-a59eef8e8b failure_category=builtin_contract requires_ref_extension=session
$name = "session_save_path";
echo function_exists($name) ? "available\n" : "missing\n";
