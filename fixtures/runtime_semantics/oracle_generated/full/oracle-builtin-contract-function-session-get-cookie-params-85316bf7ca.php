<?php
// oracle-probe: id=oracle-builtin-contract-function-session-get-cookie-params-85316bf7ca area=builtin_contract kind=function symbol=session_get_cookie_params source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-get-cookie-params-85316bf7ca failure_category=builtin_contract requires_ref_extension=session
$name = "session_get_cookie_params";
echo function_exists($name) ? "available\n" : "missing\n";
