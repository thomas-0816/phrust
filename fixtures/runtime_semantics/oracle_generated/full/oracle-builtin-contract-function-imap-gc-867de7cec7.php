<?php
// oracle-probe: id=oracle-builtin-contract-function-imap-gc-867de7cec7 area=builtin_contract kind=function symbol=imap_gc source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imap-gc-867de7cec7 failure_category=builtin_contract requires_ref_extension=imap
$name = "imap_gc";
echo function_exists($name) ? "available\n" : "missing\n";
