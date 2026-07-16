<?php
// oracle-probe: id=oracle-builtin-contract-function-imap-utf7-encode-e711071266 area=builtin_contract kind=function symbol=imap_utf7_encode source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imap-utf7-encode-e711071266 failure_category=builtin_contract requires_ref_extension=imap
$name = "imap_utf7_encode";
echo function_exists($name) ? "available\n" : "missing\n";
