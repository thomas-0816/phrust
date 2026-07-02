# WordPress Bring-Up Error Patterns

This note captures phrust gaps found while running WordPress locally against the
host phrust server and MariaDB from Docker Compose. WordPress is the application
under test and should remain unmodified; fixes belong in phrust.

## Reference callback arguments

Symptom:

- Repeated warnings from `wp-includes/class-wp-query.php`:
  `Argument #1 ($value) must be passed by reference, value given`.
- Pages could render as long warning streams even when request handling
  otherwise continued.

Pattern:

- WordPress uses callbacks that accept by-reference parameters through array
  iteration and filtering paths.
- phrust must preserve by-reference call binding semantics when invoking
  callbacks, including closures and callable arrays.

Expected coverage:

- Reduced fixtures should call a closure or callable requiring `&$value` through
  the same callback dispatch path and prove no warning is emitted.
- WordPress front page should not print callback by-reference warnings.

## Imported class names in class-constant fetches

Symptom:

- WordPress Requests transport capability checks failed or autoloaded the wrong
  class name.
- `Capability::SSL` inside a namespace was lowered using the local source name
  instead of the imported fully qualified display name.

Pattern:

- Class constants used as values or array dimensions must preserve the imported
  class display name for autoload-visible behavior.
- Lowering must not collapse non-special class names to normalized internal
  names when the PHP-visible class string matters.

Expected coverage:

- IR fixtures for both `Capability::SSL` and `isset($values[Capability::SSL])`
  under a namespace with a `use` import.
- Runtime transport probes should see Requests capability keys exactly as
  WordPress expects.

## List destructuring holes

Symptom:

- Fatal error on the homepage:
  `Unsupported operand types: array + int`.
- The failing WordPress code destructured an HTML token with a skipped list
  slot:
  `list( $token_type, , $attrs, $start_offset, $token_length ) = $next_token;`

Pattern:

- `list()` destructuring holes are positional and must not compact later
  entries.
- The syntax/HIR/lowering pipeline must preserve placeholder positions long
  enough for IR destructuring to bind numeric offsets correctly.

Expected coverage:

- A reduced fixture where `list($a, , $b, $c)` reads indexes `0`, `2`, and `3`.
- The WordPress custom CSS block support path should compute numeric offsets
  without converting an array into an arithmetic operand.

## Braced property dimension interpolation

Symptom:

- Repeated `Array to string conversion` warnings with line `0` or from
  WordPress taxonomy/query construction.
- Examples included interpolations like:
  `"{$this->rewrite['slug']}/%$this->name%"`
  and SQL clause interpolation using `$this->sql_clauses['select']`.

Pattern:

- Interpolation lowering recognized `$this->property[...]` syntax but fetched
  only the property, then converted the whole array to string.
- The dimension fetch must be emitted after the property fetch before string
  conversion.

Expected coverage:

- IR fixture proving `fetch_property` is followed by `fetch_dim` for
  `{$this->rewrite['slug']}`.
- VM fixture proving the interpolated result is the selected dimension value,
  not the whole array.
- WordPress homepage should render without `Array to string conversion`
  warnings.

## Dynamic static method calls

Symptom:

- Browser install reached Requests transport selection and failed with:
  `Fatal error: Uncaught wporg\requests\exception: No working transports found`.
- Reduced probe for:
  `$class = DynamicStaticProbe::class; $result = $class::test('ok');`
  left `$result` undefined.
- Requests code pattern:
  `$result = $class::test($capabilities);`

Pattern:

- Static method call lowering handled statically known class targets and some
  dynamic member cases, but skipped class-name variables with literal method
  names.
- Dynamic class targets should lower to a callable pair equivalent to
  `[$class, 'test']` and dispatch through callable invocation.

Expected coverage:

- IR fixture proving `$class::test()` emits a `call_callable` and assignment.
- VM fixture proving the return value is assigned and visible.
- WordPress install should complete transport selection without an undefined
  `$result` warning or a false "no transports" exception.

## Runtime cURL network gate

Symptom:

- Browser install reached the Requests cURL transport and failed with:
  `cURL error 1: network cURL requests require PHRUST_NET_TESTS=1`.
- This happened after transport selection was fixed, while WordPress performed
  install-time HTTP work through its Requests layer.

Pattern:

- phrust's cURL implementation can be intentionally network-gated for tests.
- A local WordPress server run that expects HTTP requests to execute must set
  `PHRUST_NET_TESTS=1` in the host `phrust-server` environment.
- This is runtime configuration for phrust, not a WordPress source change.

Expected coverage:

- WordPress install should be verified with the host server environment matching
  the required cURL policy.
- If the gate remains intentional, server bring-up docs or scripts should make
  the required environment explicit for local WordPress runs.

## Verification rule for WordPress bring-up

Completion is not proven by isolated probes alone. A WordPress-compatible fix
needs all of the following current-state evidence:

- Fresh WordPress install flow completes in a browser through the host
  `phrust-server`.
- The server runs directly on the host; only MariaDB runs through Docker
  Compose.
- The rendered homepage is fetched through phrust and contains the generated
  WordPress content.
- The install page and homepage do not contain `Fatal error`, `Warning:`,
  `Parse error`, `Notice:`, `Unsupported operand`, or uncaught exception output.
- Focused phrust tests cover each reduced runtime/compiler pattern fixed on the
  way to the browser result.
