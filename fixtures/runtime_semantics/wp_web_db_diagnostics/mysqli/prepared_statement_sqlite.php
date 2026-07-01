<?php
$mysqli = mysqli_connect();
if (!$mysqli) {
    echo 'connect-failed:' . mysqli_connect_errno();
    exit;
}
mysqli_query($mysqli, 'CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)');
$stmt = mysqli_prepare($mysqli, 'INSERT INTO items (name) VALUES (?)');
$name = 'alpha';
mysqli_stmt_bind_param($stmt, 's', $name);
mysqli_stmt_execute($stmt);
$name = 'beta';
mysqli_stmt_execute($stmt);

$id = 2;
$stmt = mysqli_prepare($mysqli, 'SELECT name FROM items WHERE id = ?');
mysqli_stmt_bind_param($stmt, 'i', $id);
mysqli_stmt_execute($stmt);
mysqli_stmt_bind_result($stmt, $selected);
mysqli_stmt_fetch($stmt);
echo $selected;
