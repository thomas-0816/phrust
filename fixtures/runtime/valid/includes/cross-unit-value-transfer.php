<?php
// runtime-fixture: kind=valid
class CrossUnitTransferObject {
    public $payload;
    public $public = true;
    public $defaults = array(
        'v00', 'v01', 'v02', 'v03', 'v04', 'v05', 'v06', 'v07', 'v08', 'v09',
        'v10', 'v11', 'v12', 'v13', 'v14', 'v15', 'v16', 'v17', 'v18', 'v19',
        'v20', 'v21', 'v22', 'v23', 'v24', 'v25', 'v26', 'v27', 'v28', 'v29',
        'v30', 'v31', 'v32', 'v33', 'v34', 'v35', 'v36', 'v37', 'v38', 'v39',
        'v40', 'v41', 'v42', 'v43', 'v44', 'v45', 'v46', 'v47', 'v48', 'v49',
    );
}

function cross_unit_mutate_property(&$value) {
    $value[] = 'mutated';
    return count($value);
}

require __DIR__ . '/lib/cross-unit-value-transfer-target.php';
$static_first = cross_unit_static_sequence();
require __DIR__ . '/lib/cross-unit-static-interlude.php';
cross_unit_static_interlude();
$nested_constructed = cross_unit_make_nested();

$input = array('outside' => 1);
$object = new CrossUnitTransferObject();
$result = cross_unit_transfer_values($input, $object, 'text', 41);
$returned = $result[0];
$returned['caller'] = 3;
$counter = 41;
cross_unit_increment($counter);
$named = 40;
cross_unit_named_increment(second: 2, first: $named);
$carrier = new CrossUnitTransferObject();
$carrier->payload = array('nested' => 'caller-unit-literal');
$nested = cross_unit_read_object_payload($carrier);
$default_result = cross_unit_identity($carrier->defaults);
$filtered = (new CrossUnitFilter(array('carrier' => $carrier)))
    ->filter(array('public' => true), 'and');
$dynamic = new stdClass();
$dynamic->public = true;
$dynamic_filtered = (new CrossUnitFilter(array('dynamic' => $dynamic)))
    ->filter(array('public' => true), 'and');
$referenced = array('nested' => 'caller-reference-literal');
$reference =& $referenced;
$reference_result = cross_unit_read_reference_payload($reference);
function publish_cross_unit_global() {
    global $cross_unit_global;
    $cross_unit_global = array('nested' => 'caller-global-literal');
}

publish_cross_unit_global();
$global_result = cross_unit_read_global_payload();
$constructed = new CrossUnitConstructedDefaults();
$constructed_defaults = $constructed->values();
$constructed_count = $constructed->mutateDefaults();
$static_second = cross_unit_static_sequence();

echo $input['outside'], '|', isset($input['inside']) ? '1' : '0', '|';
echo $returned['inside'], '|', $returned['caller'], '|';
echo $result[1] === $object ? 'same' : 'different', '|';
echo $result[2], '|', $result[3], '|', $result[4], '|', $counter, '|', $named, '|';
echo $nested['nested'], '|', $default_result[0], '|', $default_result[49], '|';
echo array_key_exists('carrier', $filtered) ? 'filtered' : 'missing', '|';
echo array_key_exists('dynamic', $dynamic_filtered) ? 'dynamic' : 'missing', '|';
echo $reference_result['nested'], '|', $global_result['nested'], '|';
echo $constructed_defaults['orderby'], '|', $constructed_defaults['order'], '|';
echo $constructed_defaults['parent'] === '' ? 'empty' : 'changed', '|';
echo $constructed_defaults['cache_domain'], '|', $constructed_count, '|';
echo $constructed->values()[0], '|';
echo $static_first[0] === $static_second[0] ? 'static-same' : 'static-different', '|';
echo $static_first[0]->first, '|', $static_second[1], '|';
echo $nested_constructed->value, "\n";
