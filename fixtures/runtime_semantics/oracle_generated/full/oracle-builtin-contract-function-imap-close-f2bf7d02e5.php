<?php
// oracle-probe: id=oracle-builtin-contract-function-imap-close-f2bf7d02e5 area=builtin_contract kind=function symbol=imap_close source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imap-close-f2bf7d02e5 failure_category=builtin_contract requires_ref_extension=imap
$name = "imap_close";
echo function_exists($name) ? "available\n" : "missing\n";
