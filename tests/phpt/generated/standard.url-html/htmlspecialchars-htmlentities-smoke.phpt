--TEST--
standard.url-html: default HTML escaping helpers
--FILE--
<?php
$input = "<a href='x&y'>\"Hi\"</a>";
var_dump(htmlspecialchars($input));
var_dump(htmlentities($input));
var_dump(htmlspecialchars_decode(htmlspecialchars($input)));
?>
--EXPECT--
string(58) "&lt;a href=&#039;x&amp;y&#039;&gt;&quot;Hi&quot;&lt;/a&gt;"
string(58) "&lt;a href=&#039;x&amp;y&#039;&gt;&quot;Hi&quot;&lt;/a&gt;"
string(22) "<a href='x&y'>"Hi"</a>"
