<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-start-document-824d5d426a area=builtin_contract kind=function symbol=xmlwriter_start_document source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-start-document-824d5d426a failure_category=builtin_contract requires_ref_extension=xmlwriter
try {
    $result = \xmlwriter_start_document();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
