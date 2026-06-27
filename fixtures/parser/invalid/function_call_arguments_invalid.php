<?php
// invalid: function calls cannot contain empty argument slots
foo(,);
foo($foo,,);
foo($foo,,$bar);
foo(,$foo);
