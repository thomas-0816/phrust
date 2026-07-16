<?php
// oracle-probe: id=oracle-builtin-contract-function-inflate-get-read-len-8e46614dfa area=builtin_contract kind=function symbol=inflate_get_read_len source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-inflate-get-read-len-8e46614dfa failure_category=builtin_contract requires_ref_extension=zlib
$name = "inflate_get_read_len";
echo function_exists($name) ? "available\n" : "missing\n";
