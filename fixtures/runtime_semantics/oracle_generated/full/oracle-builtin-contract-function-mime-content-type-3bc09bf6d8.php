<?php
// oracle-probe: id=oracle-builtin-contract-function-mime-content-type-3bc09bf6d8 area=builtin_contract kind=function symbol=mime_content_type source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mime-content-type-3bc09bf6d8 failure_category=builtin_contract requires_ref_extension=fileinfo
try {
    $result = \mime_content_type();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
