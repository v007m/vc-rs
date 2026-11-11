"""A reporter plugin, standard library only: it counts the run's events, reads the secret its
manifest declares, is refused one it does not, and returns one report file."""
from rpc import read, send, serve

events = []


def secret(name):
    send({"id": "s-" + name, "method": "secrets/get", "params": {"name": name}})
    while True:
        message = read()
        if message is None or message.get("id") == "s-" + name:
            return message or {"error": {"code": -32603, "message": "the host left"}}


def report(summary):
    token = secret("reportToken")
    refused = secret("apiKey")
    lines = [
        "status " + summary["status"],
        "events " + str(len(events)),
        "reportToken " + str(len(token.get("result", {}).get("value", ""))) + " chars",
        "apiKey " + ("refused " + str(refused["error"]["code"]) if "error" in refused else "given"),
    ]
    return {"files": [{"name": "plugin/report.txt", "content": "\n".join(lines) + "\n"}]}


def answer(method, params):
    if method == "reporter/onEvent":
        events.append(params["event"]["event"])
        return None
    if method == "reporter/finish":
        return report(params["summary"])
    return None


serve(answer)
