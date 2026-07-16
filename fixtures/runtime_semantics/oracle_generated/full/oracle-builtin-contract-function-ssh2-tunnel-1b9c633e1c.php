<?php
// oracle-probe: id=oracle-builtin-contract-function-ssh2-tunnel-1b9c633e1c area=builtin_contract kind=function symbol=ssh2_tunnel source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ssh2-tunnel-1b9c633e1c failure_category=builtin_contract requires_ref_extension=ssh2
$name = "ssh2_tunnel";
echo function_exists($name) ? "available\n" : "missing\n";
