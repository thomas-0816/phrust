<?php
// oracle-probe: id=oracle-builtin-contract-function-msgpack-unpack-584101dd56 area=builtin_contract kind=function symbol=msgpack_unpack source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msgpack-unpack-584101dd56 failure_category=builtin_contract requires_ref_extension=msgpack
$name = "msgpack_unpack";
echo function_exists($name) ? "available\n" : "missing\n";
