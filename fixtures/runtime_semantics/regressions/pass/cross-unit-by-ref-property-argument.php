<?php
// runtime-semantics: expect=pass regression_category=runtime_dispatch reference_behavior=property_lvalue_is_passed_by_reference regression_case=cross_unit_by_ref_property_argument

require __DIR__ . '/../_data/cross-unit-by-ref-property-provider.php';

class CrossUnitReferenceHolder {
    public array $items = ['seed'];

    public function update(): void {
        append_cross_unit_marker($this->items);
    }
}

$holder = new CrossUnitReferenceHolder();
$holder->update();
var_dump($holder->items);

$by_value_holder = new CrossUnitReferenceHolder();
$changed_copy = append_to_cross_unit_copy($by_value_holder->items);
var_dump($by_value_holder->items, $changed_copy);
