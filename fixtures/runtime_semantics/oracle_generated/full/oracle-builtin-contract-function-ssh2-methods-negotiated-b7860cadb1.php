<?php
// oracle-probe: id=oracle-builtin-contract-function-ssh2-methods-negotiated-b7860cadb1 area=builtin_contract kind=function symbol=ssh2_methods_negotiated source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ssh2-methods-negotiated-b7860cadb1 failure_category=builtin_contract requires_ref_extension=ssh2
$name = "ssh2_methods_negotiated";
echo function_exists($name) ? "available\n" : "missing\n";
