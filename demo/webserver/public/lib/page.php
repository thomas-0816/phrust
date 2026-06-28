<?php

function demo_title($title)
{
    echo "<!doctype html>\n";
    echo "<html><head><meta charset=\"utf-8\"><title>", htmlspecialchars($title), "</title></head>\n";
    echo "<body style=\"font-family: system-ui, sans-serif; max-width: 760px; margin: 40px auto; line-height: 1.45;\">\n";
    echo "<h1>", htmlspecialchars($title), "</h1>\n";
}

function demo_footer()
{
    echo "<hr>\n";
    echo "<p><a href=\"/\">Back to demo index</a></p>\n";
    echo "</body></html>\n";
}

function demo_value($value)
{
    return htmlspecialchars((string) $value);
}
