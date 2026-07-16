<?php
// oracle-probe: id=oracle-builtin-contract-function-ssh2-scp-send-858e3e4a4c area=builtin_contract kind=function symbol=ssh2_scp_send source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ssh2-scp-send-858e3e4a4c failure_category=builtin_contract requires_ref_extension=ssh2
$name = "ssh2_scp_send";
echo function_exists($name) ? "available\n" : "missing\n";
