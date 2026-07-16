<?php
// oracle-probe: id=oracle-builtin-contract-function-simplexml-import-dom-ea456c4b6e area=builtin_contract kind=function symbol=simplexml_import_dom source=ext/simplexml/simplexml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-simplexml-import-dom-ea456c4b6e failure_category=builtin_contract requires_ref_extension=simplexml
try {
    $result = \simplexml_import_dom();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
