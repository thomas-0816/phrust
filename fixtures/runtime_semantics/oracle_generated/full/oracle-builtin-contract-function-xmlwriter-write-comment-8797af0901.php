<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-write-comment-8797af0901 area=builtin_contract kind=function symbol=xmlwriter_write_comment source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-write-comment-8797af0901 failure_category=builtin_contract requires_ref_extension=xmlwriter
$name = "xmlwriter_write_comment";
echo function_exists($name) ? "available\n" : "missing\n";
