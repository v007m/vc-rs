"""An auth plugin, standard library only: `auth/credentials` answers with a header built from
the auth definition's config, a session cookie, and how many times it was asked."""
from rpc import serve

calls = 0


def credentials(params):
    global calls
    calls += 1
    config = params.get("config") or {}
    return {
        "headers": {"x-stamp": config["tenant"] + ":" + config["key"], "x-calls": str(calls)},
        "cookies": {"session": "s-" + params["auth"]},
    }


def answer(method, params):
    if method == "auth/credentials":
        return credentials(params)
    if method == "initialize":
        return {}
    return None


serve(answer)
