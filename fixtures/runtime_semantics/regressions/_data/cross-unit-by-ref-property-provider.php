<?php

function append_cross_unit_marker(array &$items): void {
    $items[] = 'updated';
}

function append_to_cross_unit_copy(array $items): array {
    $items[] = 'copy';
    return $items;
}
