# Optional script extensions

Filesystem, process/environment, and network functions are not part of a bare
`ScriptEngine`. Each must be compiled and enabled in `ExtensionConfig`. The CLI
enables every compiled extension for the local script unless disabled with
`--no-extensions`, `--no-fs`, `--no-exec`, or `--no-net`.

Code submitted to `POST /eval` is extension-free by default even while the local
script has extensions. `--api-allow-extensions` opts remote evaluation into the
same configured extensions. This is a security boundary, not a type-system
difference.

Registration source:
[`extensions/mod.rs`](../../crates/vibelang-rhai/src/extensions/mod.rs).

## Filesystem (`ext-fs`)

| Exact signature | Return |
|---|---|
| `read_file(path: String)` | String |
| `read_lines(path: String)` | Array of String |
| `write_file(path: String, content: String)`; `append_file(...)` | Unit or Rhai I/O error |
| `file_exists(path)`; `is_dir(path)`; `is_file(path)` | Bool |
| `file_size(path)` | Int bytes or error |
| `list_dir(path)` | Array of String |
| `create_dir(path)`; `create_dir_all(path)`; `remove_dir(path)`; `remove_file(path)` | Unit or error |
| `copy_file(source: String, destination: String)` | Int bytes copied |
| `rename_file(source, destination)` | Unit or error |
| `glob(pattern: String)` | Array of relative paths |
| `path_join(a: String, b: String)` | String |
| `path_parent(path)`; `path_filename(path)`; `path_extension(path)`; `path_stem(path)` | String |

`--fs-sandbox PATH` configures a base. Absolute paths and canonicalized existing
traversal are rejected. A current security gap remains: a nonexistent
destination containing traversal cannot be canonicalized before creation and
may escape the base. Do not treat this as a hardened hostile-code sandbox.

Existence predicates swallow path-resolution errors as false. `list_dir`
silently drops unreadable/non-Unicode entries. `glob` implements `*`, `**`, and
`?` itself by recursively walking the base/current directory. Source:
[`fs.rs`](../../crates/vibelang-rhai/src/extensions/fs.rs).

## Process and environment (`ext-exec`)

| Exact signature | Return / behavior |
|---|---|
| `exec(command: String)` | Stdout String |
| `exec_status(command: String)` | Int exit code |
| `exec_lines(command: String)` | Array of stdout lines |
| `exec_with_args(program: String, args: Array)` | Stdout; each Dynamic is stringified |
| `exec_full(command: String)` | Map `{stdout,stderr,status,success}` |
| `shell(script: String)` | Stdout through `/bin/sh -c` or Windows `cmd` |
| `env_var(name: String)` | String; missing/non-Unicode is empty |
| `env_var_or(name: String, fallback: String)` | String |
| `set_env_var(name: String, value: String)` | Unit |
| `env_vars()` | Map |
| `cwd()` | String |
| `set_cwd(path: String)` | Unit or error |
| `pid()` | Int |

`exec`, `exec_status`, `exec_lines`, and `exec_full` split on whitespace and do
not invoke a shell. Empty command returns empty stdout/status 0/success. Launch
failures error, but a nonzero process exit does not; stdout-only calls discard
stderr. Environment and working-directory mutation affects the entire VibeLang
process. Source: [`exec.rs`](../../crates/vibelang-rhai/src/extensions/exec.rs).

## Network, URL, and JSON (`ext-net`)

| Exact signature | Return |
|---|---|
| `http_get(url: String)` | Body String |
| `http_get_lines(url: String)` | Array of lines |
| `http_get_json(url: String)` | Dynamic parsed JSON |
| `http_post(url: String, body: String)` | Body String; form content type |
| `http_post_json(url: String, body: Map)` | Dynamic parsed JSON |
| `url_encode(value: String)`; `url_decode(value: String)` | String |
| `parse_url(url: String)` | Map of parsed components |
| `build_query_string(values: Map)` | String |
| `json_parse(text: String)` | Dynamic |
| `json_stringify(value: Dynamic)` | String |

The client is blocking raw TCP with 30-second read/write timeouts, HTTP/1.1
close, no redirect or chunked decoding, and no status-code validation. It
discards response status and headers. `https` syntax parses but always errors;
there is no registered TLS option. Host parsing is simplistic and does not
fully cover IPv6/userinfo. The handwritten JSON parser is not a full serde JSON
implementation. Invalid percent escapes are retained lossily. Source:
[`net.rs`](../../crates/vibelang-rhai/src/extensions/net.rs).

## Safe enabling example

```bash
# Local filesystem reads/writes restricted to the project; process and network off.
vibe run song.vibe --no-exec --no-net --fs-sandbox "$PWD"
```

Avoid `--api-allow-extensions` on an unauthenticated or non-loopback HTTP server.
