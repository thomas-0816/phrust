<?php
$table = get_html_translation_table(0, 3, 'UTF-8');
echo count($table), '|', $table['&'], '|', $table["'"], '|', $table['>'], '|', $table['<'], '|', $table['"'], "\n";

$minimal = get_html_translation_table(0, 0);
echo count($minimal), '|', $minimal['&'], '|', $minimal['>'], '|', $minimal['<'], "\n";
