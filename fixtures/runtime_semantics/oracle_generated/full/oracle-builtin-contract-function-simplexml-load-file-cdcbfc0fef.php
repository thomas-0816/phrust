<?php
// oracle-probe: id=oracle-builtin-contract-function-simplexml-load-file-cdcbfc0fef area=builtin_contract kind=function symbol=simplexml_load_file source=ext/simplexml/simplexml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-simplexml-load-file-cdcbfc0fef failure_category=builtin_contract requires_ref_extension=simplexml
try {
    $result = \simplexml_load_file();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
