# Plugin protocol

A Vibranium plugin is a separate program that the engine starts and talks to over its stdin and stdout. This page is the contract a plugin author writes against. The LLD's "Plugins" section describes how the host side is built; `syntax-reference.md` describes the `plugins:` config and `vibranium.lock`.

## Install and approval

`vc plugin add <source>` fetches a plugin, shows the permissions its manifest asks for, and installs it once they are approved.

- **Sources.** `npm:<package>[@<range>]` (packed with `npm pack`) or `path:<folder>` (a folder relative to the project root). No other scheme is accepted.
- **Where.** The plugin is installed to `.vibranium/plugins/<id>/`.
- **Lock.** `vibranium.lock` (YAML) records each approved plugin: its source, resolved version, integrity hash and the permissions granted. The host refuses to start a plugin whose manifest asks for more than the lock grants; run `vc plugin add` again to approve the new permissions. For a `path:` plugin the integrity is a `blake3-…` hash over the installed files (`.git`, `.vibranium` and `__pycache__` skipped), checked again on the plugin's first call in a run (off the async threads, and only for a plugin the run uses): files changed since the lock approved them fail its calls (V5118) until `vc plugin add` approves them again. A plugin must not write into its install folder; it keeps its own files in its data folder (below). An npm integrity is the tarball's and is checked when it is fetched. Installed files keep their permissions, so an executable entry stays executable.

## Manifest: `vibranium-plugin.yaml`

Every plugin has a manifest at the top of its folder.

```yaml
id: acmeReport           # camelCase; the name used in the config
version: 1.2.0
kind: reporter           # reporter | auth | importer | executor
entry:
  command: python3       # a file in the plugin folder, or a program on PATH
  args: [report.py]
permissions:
  network: [api.acme.dev]   # hosts it may reach
  secrets: [reportToken]    # secrets it may ask for with secrets/get
  files: [reports/]         # project paths it may read or write
```

`importer` and `executor` kinds are accepted in the manifest, but no host methods exist for them yet.

## Transport

- JSON-RPC 2.0, one process per plugin per run, started on first use in the plugin's folder.
- Each message is framed as in LSP: a `Content-Length: <bytes>` header, a blank line (`\r\n\r\n`), then the UTF-8 JSON body. Other headers are ignored. A message is at most 16 MiB.
- stdout carries only protocol messages. stderr is passed through to the user's terminal.
- A message that is not a framed JSON-RPC object stops the plugin for the rest of the run: the calls in flight fail, and so does every later call.
- The plugin must read its stdin: a call it does not read within the call's deadline stops it the same way. The host holds at most 1023 of its messages unread; a plugin that writes more than that before the host reads them is stopped too. A stopped plugin's process is killed.
- The process gets only `PATH`, `HOME`, `USER`, `LANG`, `TMPDIR`, `TEMP`, `TMP`, `SYSTEMROOT` and `USERPROFILE` from the environment, plus `VIB_PLUGIN_DATA`: the absolute path of its data folder, `.vibranium/plugin-data/<id>/`, made before it starts; when a folder on the way to it is a link or not a folder, the plugin is not started. A plugin writes its state, caches and logs there, never into its install folder, which is checked against the lock. It never gets the run's secrets through the environment.

## Lifecycle

1. **`initialize`** (host → plugin, deadline 5 s), with params `{"protocolVersion": 1, "plugin": "<id>", "config": <plugins[].config or null>}`. Any result means the plugin is ready. An error or timeout fails every call to this plugin in the run. The host starts the process on the first call a run makes to it, so a plugin the run never uses never starts.

`plugins[].config` holds literal values only. A `${…}` in it makes the plugin unusable for the run, with a notice; a plugin reads a secret with `secrets/get`, so the lock decides which ones it gets.
2. Calls for the plugin's kind (below), each with a 30 s deadline.
3. **`shutdown`** (host → plugin, no params) when the run ends. After it, the host closes stdin. A plugin that has not exited within 1 s of each step is killed.

A plugin that crashes or exceeds a deadline fails only the work that used it: the reporter's output, or the requests using the auth. Later calls to a stopped plugin fail at once.

## Methods the host calls

| Kind | Method | Params | Result |
| --- | --- | --- | --- |
| reporter | `reporter/onEvent` | `{"event": <event>}`, in the form the `json` console format prints | ignored |
| reporter | `reporter/finish` | `{"summary": <run summary>}` | `{"files": [{"name": "report.html", "content": "<text>"}]}` |
| auth | `auth/credentials` | `{"auth": "<auth id>", "config": <the auth's config or null>}` | `{"headers": {…}, "query": {…}, "cookies": {…}, "expiresIn": <seconds>}` |

- **Reporter events.** The host queues events (up to 1024) and sends them to `reporter/onEvent` one at a time, in order, without ever holding up the run: a plugin that falls a whole queue behind gets no more events, its report is not written, and the run names it once in a notice. `reporter/finish` is sent once every queued event has been answered.
- **Reporter files.** Each file is saved in the run's report folder. A `name` must be a relative path that stays inside that folder; any other name fails the reporter.
- **Auth config.** Unlike `plugins[].config`, an auth definition's `config` is rendered like any other auth setting, so it may hold `${secret.x}` or `${var.x}`. A secret written there is sent to the plugin whatever the lock grants: writing it into the auth definition is the grant. Every secret is in the redactor's set from the start of the run, so it never appears in events, logs or history.
- **Auth results.** At least one of `headers`, `query` or `cookies` must hold a value. Every value is treated as a secret and is redacted in output. Without `expiresIn`, the credentials are kept for the run.

## Methods the plugin calls

| Method | Params | Result |
| --- | --- | --- |
| `secrets/get` | `{"name": "<secret>"}` | `{"value": "<text>"}` |

The host answers `secrets/get` only for a secret that the manifest declares, the lock grants and the run has. Otherwise it returns error −32001.

## Errors

| Code | Meaning |
| --- | --- |
| −32001 | Permission denied: a secret, or a start-up, that the lock does not grant |
| −32002 | Deadline passed (set by the host when a call times out) |
| −32601 | Method not found |
| −32603 | Internal error (also used for an error object without a code) |

Every plugin failure is V5118 and names the plugin, the method and the JSON-RPC code. A reporter plugin's failure becomes a run notice, and the plugin gets no more events; an auth plugin's failure fails the requests that use it, as V5114 like any other auth failure. A plugin the run cannot use (not installed, not approved, asking for more than the lock grants, or with a `${…}` in its `config`) is named in a run notice with the `vc plugin` command that fixes it, and an auth that names it fails with that reason. A plugin whose files changed since the lock approved them fails its first call with V5118 and the same fix.

## Using a plugin

```yaml
# vibranium.config.yaml
syntaxVersion: 2
profiles:
  - id: dev
    default: true
plugins:
  - id: acmeReport
    source: npm:@acme/vibranium-report@^1
    config: { theme: dark }
  - id: acmeSso
    source: npm:@acme/vibranium-sso@^2
reports:
  formats: [junit, acmeReport]     # a reporter plugin, by its ID

auth:
  - id: partnerSso
    type: plugin
    plugin: acmeSso                # an auth plugin, by its ID
    config: { tenant: eu }         # passed to `auth/credentials`
```
