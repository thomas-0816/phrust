<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-open-memory-a2aeb9e02b area=builtin_contract kind=function symbol=xmlwriter_open_memory source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-open-memory-a2aeb9e02b failure_category=builtin_contract requires_ref_extension=xmlwriter
try {
    $result = \xmlwriter_open_memory(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
