<?php
// oracle-probe: id=oracle-internal-api-contract-method-random-engine-pcgoneseq128xslrr64-serialize-9484c8d193 area=internal_api_contract kind=method symbol=Random\Engine\PcgOneseq128XslRr64::__serialize source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-random-engine-pcgoneseq128xslrr64-serialize-9484c8d193 failure_category=internal_api_contract requires_ref_extension=random
$class = "Random\\Engine\\PcgOneseq128XslRr64";
$member = "__serialize";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
