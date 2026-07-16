<?php
// oracle-probe: id=oracle-builtin-contract-function-ssh2-auth-none-67d61f0999 area=builtin_contract kind=function symbol=ssh2_auth_none source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ssh2-auth-none-67d61f0999 failure_category=builtin_contract requires_ref_extension=ssh2
$name = "ssh2_auth_none";
echo function_exists($name) ? "available\n" : "missing\n";
