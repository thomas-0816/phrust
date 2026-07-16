<?php
// oracle-probe: id=oracle-builtin-contract-function-dom-import-simplexml-f415684eab area=builtin_contract kind=function symbol=dom_import_simplexml source=ext/dom/php_dom.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-dom-import-simplexml-f415684eab failure_category=builtin_contract requires_ref_extension=dom
try {
    $result = \dom_import_simplexml();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
