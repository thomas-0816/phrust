<?php
// oracle-probe: id=oracle-internal-api-contract-method-xmlwriter-enddocument-a6061c595f area=internal_api_contract kind=method symbol=XMLWriter::endDocument source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-xmlwriter-enddocument-a6061c595f failure_category=internal_api_contract requires_ref_extension=xmlwriter
$class = "XMLWriter";
$member = "endDocument";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
