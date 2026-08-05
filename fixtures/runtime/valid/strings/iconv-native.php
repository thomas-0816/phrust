<?php
var_dump(iconv('UTF-8', 'ASCII//TRANSLIT', 'Héllo'));
var_dump(iconv('UTF-8', 'ASCII//IGNORE', 'A€B'));
var_dump(iconv('UTF-8', 'UTF-8', 'native'));
