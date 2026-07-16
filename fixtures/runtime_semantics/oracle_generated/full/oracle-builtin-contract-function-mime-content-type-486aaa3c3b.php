<?php
// oracle-probe: id=oracle-builtin-contract-function-mime-content-type-486aaa3c3b area=builtin_contract kind=function symbol=mime_content_type source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mime-content-type-486aaa3c3b failure_category=builtin_contract requires_ref_extension=fileinfo
$name = "mime_content_type";
echo function_exists($name) ? "available\n" : "missing\n";
