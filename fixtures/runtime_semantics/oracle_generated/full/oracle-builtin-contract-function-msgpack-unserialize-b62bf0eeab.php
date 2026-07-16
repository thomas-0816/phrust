<?php
// oracle-probe: id=oracle-builtin-contract-function-msgpack-unserialize-b62bf0eeab area=builtin_contract kind=function symbol=msgpack_unserialize source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msgpack-unserialize-b62bf0eeab failure_category=builtin_contract requires_ref_extension=msgpack
$name = "msgpack_unserialize";
echo function_exists($name) ? "available\n" : "missing\n";
