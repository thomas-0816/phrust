<?php
// oracle-probe: id=oracle-builtin-contract-function-imap-last-error-8a0d799971 area=builtin_contract kind=function symbol=imap_last_error source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imap-last-error-8a0d799971 failure_category=builtin_contract requires_ref_extension=imap
$name = "imap_last_error";
echo function_exists($name) ? "available\n" : "missing\n";
