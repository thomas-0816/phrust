<?php

function lazy_cross_unit_labels($taxonomy) {
    $taxonomy->labels = (array) $taxonomy->labels;

    $defaults = LazyCrossUnitTaxonomy::get_default_labels();
    $defaults['menu_name'] = $defaults['name'];
    $labels = (object) $defaults;
    $taxonomy_name = $taxonomy->name;
    $labels->taxonomy_name = $taxonomy_name;
    $default_labels = clone $labels;
    $labels = lazy_cross_unit_apply_filters("taxonomy_labels_{$taxonomy_name}", $labels);
    $labels = (object) array_merge((array) $default_labels, (array) $labels);

    return $labels;
}
