<?php
// oracle-probe: id=oracle-builtin-contract-function-imap-utf7-decode-2582878379 area=builtin_contract kind=function symbol=imap_utf7_decode source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imap-utf7-decode-2582878379 failure_category=builtin_contract requires_ref_extension=imap
$name = "imap_utf7_decode";
echo function_exists($name) ? "available\n" : "missing\n";
