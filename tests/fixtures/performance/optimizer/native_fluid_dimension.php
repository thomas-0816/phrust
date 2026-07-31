<?php

function merge_dimension_options(array $options, array $defaults): array
{
    return array_merge($defaults, $options);
}

function normalize_dimension($raw_value, $options = array())
{
    if (!is_string($raw_value) && !is_int($raw_value) && !is_float($raw_value)) {
        return null;
    }
    if (empty($raw_value)) {
        return null;
    }
    if (is_numeric($raw_value)) {
        $raw_value = $raw_value . 'px';
    }

    $defaults = array(
        'coerce_to' => '',
        'root_size_value' => 16,
        'acceptable_units' => array('rem', 'px', 'em'),
    );
    $options = merge_dimension_options($options, $defaults);
    $acceptable_units_group = implode('|', $options['acceptable_units']);
    $pattern = '/^(\d*\.?\d+)(' . $acceptable_units_group . '){1,1}$/';
    preg_match($pattern, $raw_value, $matches);
    if (!isset($matches[1]) || !isset($matches[2])) {
        return null;
    }

    $value = $matches[1];
    $unit = $matches[2];
    if ('px' === $options['coerce_to'] && ('em' === $unit || 'rem' === $unit)) {
        $value = $value * $options['root_size_value'];
        $unit = $options['coerce_to'];
    }
    if ('px' === $unit && ('em' === $options['coerce_to'] || 'rem' === $options['coerce_to'])) {
        $value = $value / $options['root_size_value'];
        $unit = $options['coerce_to'];
    }
    if (
        ('em' === $options['coerce_to'] || 'rem' === $options['coerce_to'])
        && ('em' === $unit || 'rem' === $unit)
    ) {
        $unit = $options['coerce_to'];
    }
    return array(
        'value' => round($value, 3),
        'unit' => $unit,
    );
}

function compute_fluid_dimension($args = array())
{
    $maximum_viewport_width_raw = isset($args['maximum_viewport_width']) ? $args['maximum_viewport_width'] : null;
    $minimum_viewport_width_raw = isset($args['minimum_viewport_width']) ? $args['minimum_viewport_width'] : null;
    $maximum_font_size_raw = isset($args['maximum_font_size']) ? $args['maximum_font_size'] : null;
    $minimum_font_size_raw = isset($args['minimum_font_size']) ? $args['minimum_font_size'] : null;
    $scale_factor = isset($args['scale_factor']) ? $args['scale_factor'] : null;

    $minimum_font_size = normalize_dimension($minimum_font_size_raw);
    $font_size_unit = isset($minimum_font_size['unit']) ? $minimum_font_size['unit'] : 'rem';
    $maximum_font_size = normalize_dimension(
        $maximum_font_size_raw,
        array('coerce_to' => $font_size_unit)
    );
    if (!$maximum_font_size || !$minimum_font_size) {
        return null;
    }

    $minimum_font_size_rem = normalize_dimension(
        $minimum_font_size_raw,
        array('coerce_to' => 'rem')
    );
    $maximum_viewport_width = normalize_dimension(
        $maximum_viewport_width_raw,
        array('coerce_to' => $font_size_unit)
    );
    $minimum_viewport_width = normalize_dimension(
        $minimum_viewport_width_raw,
        array('coerce_to' => $font_size_unit)
    );
    if (!$minimum_viewport_width || !$maximum_viewport_width) {
        return null;
    }

    $linear_factor_denominator = $maximum_viewport_width['value'] - $minimum_viewport_width['value'];
    if (empty($linear_factor_denominator)) {
        return null;
    }
    $view_port_width_offset = round($minimum_viewport_width['value'] / 100, 3) . $font_size_unit;
    $linear_factor = 100 * (
        ($maximum_font_size['value'] - $minimum_font_size['value'])
        / $linear_factor_denominator
    );
    $linear_factor_scaled = round($linear_factor * $scale_factor, 3);
    $linear_factor_scaled = empty($linear_factor_scaled) ? 1 : $linear_factor_scaled;
    $fluid_target_font_size = implode('', $minimum_font_size_rem)
        . " + ((1vw - $view_port_width_offset) * $linear_factor_scaled)";

    return "clamp($minimum_font_size_raw, $fluid_target_font_size, $maximum_font_size_raw)";
}

$cases = array(
    array('minimum_font_size' => '1rem', 'maximum_font_size' => '1.125rem'),
    array('minimum_font_size' => '1.125rem', 'maximum_font_size' => '1.375rem'),
    array('minimum_font_size' => '1.75rem', 'maximum_font_size' => '2rem'),
);
foreach ($cases as $case) {
    $case['minimum_viewport_width'] = '320px';
    $case['maximum_viewport_width'] = '1340px';
    $case['scale_factor'] = 1;
    echo compute_fluid_dimension($case), "\n";
}
