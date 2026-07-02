<?php
// runtime-semantics: requires_ref_extension=mysqli
$mysqli = new mysqli();
if (!$mysqli->real_connect()) {
    echo 'connect-failed:' . $mysqli->connect_errno;
    exit;
}
$mysqli->query('CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)');
$mysqli->query("INSERT INTO items (name) VALUES ('alpha')");
$result = $mysqli->query('SELECT id, name FROM items');
$assoc = $result->fetch_assoc();
echo $assoc['name'];
echo "\n";
echo $result->num_rows;
echo "\n";
echo $mysqli->character_set_name();
