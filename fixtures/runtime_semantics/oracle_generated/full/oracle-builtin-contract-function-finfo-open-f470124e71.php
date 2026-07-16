<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-open-f470124e71 area=builtin_contract kind=function symbol=finfo_open source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-open-f470124e71 failure_category=builtin_contract requires_ref_extension=fileinfo
try {
    $result = \finfo_open(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
