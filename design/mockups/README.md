# UI mockups

The live, clickable version is the design canvas at https://claude.ai/artifact/6LkEaddcAAZ5Fs78L8NLL6 (version 31). It has four pages: CLI, Report, TUI and Web. Use Play mode to click through. The files here are its sources, kept as the visual spec for M8 (TUI) and M9 (web UI).

| Path | Contents |
| --- | --- |
| `canvas/project/*.dc.html`, `canvas.json` | One artboard per screen. CLI: `CliStart`, `CliAuthor`, `CliRun`, `CliMaintain`, `CliCheck`, `CliNew`, `CliHar`. Report: `Main`, `ReportFailure`, `ReportLoad`. TUI: `TuiRun`, `TuiExplore`, `TuiHistory`, `TuiImport`. Web: `WebExplorer`, `WebClient`, `WebEditor`, `WebSettings`, `WebRun`, `WebInsights`, `WebAgents`. |
| `canvas/vib-web2.css`, `noise.png`, `js-yaml.min.js` | The shared stylesheet, the grain texture and the YAML parser. In the canvas they are referenced as `/_blob/81dcd3b549d078b575adb6aed82e1861`, `/_blob/9ea3af80b64659e41e7d942696a9a0d2` and `/_blob/5809f53fc2a29e786d4c63f8d536045b`. |
| `generators/editor-src` | Source of the `WebEditor` artboard: form, YAML and split views over one text buffer, minimal-diff form edits, and an LCS diff. Build with `node build.js`. |
| `generators/client-src` | Source of the `WebClient` artboard (the Postman-style client with multi-stage flows and HAR import). Build with `build.py`. |
| `generators/settings-src` | Source of `WebSettings`. Build with `python3 build.py 990`. |
| `generators/gen_cli.py`, `gen_cli2.py` | Generators for the CLI and TUI terminal screens. |
| `generators/brief-common.md`, `brief-shell-v2.md` | The design brief: the look (gradient with grain, flat chrome, one elevated panel, Geist), the shell, responsiveness rules, and the facts the Settings and Agents screens show. |
| `tests/` | Node logic tests for the editor and client (`editor-tests/run-all.sh`) and the screenshot and overflow audits used at widths 1440, 1180, 960, 760 and 390. Screenshots were not copied. |

Several build and test scripts still use absolute paths from the original cloud scratchpad (`/tmp/claude-0/...`). Change those paths before running them. The logic worth porting to Svelte in M9 is the editor's model in `editor-src`: each file keeps `original`, `text` and `model`; switching views never rewrites the text; form edits are minimal line edits; and diff is a real LCS diff.
