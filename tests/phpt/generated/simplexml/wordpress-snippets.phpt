--TEST--
simplexml: WordPress-style RSS, plugin metadata, and config snippets
--DESCRIPTION--
Generated SimpleXML coverage for common WordPress/framework XML probing patterns.
--EXTENSIONS--
simplexml
--FILE--
<?php
$rss = simplexml_load_string('<rss><channel><title>Feed</title><item><title>Post</title></item></channel></rss>');
echo $rss->channel->title, "|", $rss->channel->item->title, "\n";

$plugin = simplexml_load_string('<plugin slug="demo"><name>Demo Plugin</name><version>1.2.3</version></plugin>');
$attrs = $plugin->attributes();
echo $attrs->slug, "|", $plugin->name, "|", $plugin->version, "\n";

$config = simplexml_load_string('<config><option name="uploads">enabled</option></config>');
foreach ($config as $name => $value) {
    echo $name, ":", $value->attributes()->name, "=", $value, "\n";
}
?>
--EXPECT--
Feed|Post
demo|Demo Plugin|1.2.3
option:uploads=enabled
