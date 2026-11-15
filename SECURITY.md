# Security policy

## Reporting a vulnerability

Report vulnerabilities privately, through GitHub's private vulnerability reporting: open the repository's [Security tab](https://github.com/v007m/vc-rs/security) and choose **Report a vulnerability** (or go straight to [the form](https://github.com/v007m/vc-rs/security/advisories/new)). Only the maintainers see the report.

Don't open a public issue, pull request or discussion about a vulnerability before it is fixed. If the private form isn't available, open an issue that asks the maintainer, [@sarathm09](https://github.com/sarathm09), for a private channel, and leave every detail of the problem out of it.

A useful report has:

- the `vc --version` output, the OS and the install method;
- what an attacker can do, and what they need first (a malicious test file, plugin, OpenAPI spec or HAR file, a hostile server, local access);
- the smallest project or steps that show it, with real secrets removed.

Things in scope include a secret that reaches output, a report, history or a log despite redaction; a plugin that gets past its declared permissions; an agent (MCP) call that skips the approval policy; the local web UI (`vc ui`) or MCP endpoint being reachable or callable from another origin or host; and code execution from a file `vc` only reads.

## Supported versions

Vibranium is before 1.0. Fixes go into the latest release line only.

| Version | Gets security fixes |
| --- | --- |
| 0.1.x | Yes |
| Older or unreleased builds of `master` | No: upgrade to the latest 0.1.x |

## What to expect

- **Acknowledgement** within 7 days of the report.
- **An assessment** within 14 days: whether it is a vulnerability, how severe it is, and the plan.
- **A fix** in a 0.1.x patch release, as soon as one is ready, with a GitHub security advisory that credits you unless you'd rather not be named.
- **Disclosure** is coordinated with you. We ask that you keep the details private for up to 90 days after the report, or until the fix is released, whichever comes first.

Vibranium is maintained by volunteers, so these are targets, not guarantees. If you hear nothing within 7 days, follow up on the same report.
