<?php

function build_clauses(array &$query): array
{
    $chunks = array(
        'join' => array(),
        'where' => array(),
    );
    $sql = array(
        'join' => '',
        'where' => '',
    );

    foreach ($query as &$clause) {
        $chunks['join'][] = $clause['join'];
        $chunks['where'][] = $clause['where'];
    }

    $chunks['join'] = array_filter($chunks['join']);
    $chunks['where'] = array_filter($chunks['where']);
    if (!empty($chunks['join'])) {
        $sql['join'] = implode(' ', array_unique($chunks['join']));
    }
    if (!empty($chunks['where'])) {
        $sql['where'] = implode(' AND ', $chunks['where']);
    }
    return $sql;
}

function build_recursive_clauses(array &$query): array
{
    $chunks = array('join' => array(), 'where' => array());
    $sql = array('join' => '', 'where' => '');
    foreach ($query as &$clause) {
        if (isset($clause['children'])) {
            $child = build_recursive_clauses($clause['children']);
            $chunks['join'][] = $child['join'];
            $chunks['where'][] = $child['where'];
        } else {
            $chunks['join'][] = $clause['join'];
            $chunks['where'][] = $clause['where'];
        }
    }
    $chunks['join'] = array_filter($chunks['join']);
    $chunks['where'] = array_filter($chunks['where']);
    if (!empty($chunks['join'])) {
        $sql['join'] = implode(' ', array_unique($chunks['join']));
    }
    if (!empty($chunks['where'])) {
        $sql['where'] = '( ' . implode(' AND ', $chunks['where']) . ' )';
    }
    return $sql;
}

$query = array(
    array('join' => 'LEFT JOIN terms', 'where' => 'term_id = 1'),
    array('join' => 'LEFT JOIN terms', 'where' => 'term_id = 2'),
);
$sql = build_clauses($query);
echo gettype($sql['join']), '|', $sql['join'], "\n";
echo gettype($sql['where']), '|', $sql['where'], "\n";

$recursive = array(
    array(
        'children' => array(
            array('join' => 'INNER JOIN posts', 'where' => 'post_id = 7'),
            array('join' => 'INNER JOIN posts', 'where' => 'post_id = 8'),
        ),
    ),
);
$sql = build_recursive_clauses($recursive);
echo gettype($sql['join']), '|', $sql['join'], "\n";
echo gettype($sql['where']), '|', $sql['where'], "\n";
