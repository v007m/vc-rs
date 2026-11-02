import html, re, sys, os

OUT = sys.argv[1]
MARK = re.compile(r"«([a-z ]+)\|(.*?)»")


def vis(s):
    return len(MARK.sub(lambda m: m.group(2), s))


def pad(s, w):
    return s + " " * max(0, w - vis(s))


def row(cells, widths):
    out = ""
    for i, c in enumerate(cells):
        out += pad(c, widths[i]) if i < len(widths) else c
    return out.rstrip()


def render(s):
    out, pos = "", 0
    for m in MARK.finditer(s):
        out += html.escape(s[pos:m.start()], quote=False)
        out += '<span class="%s">%s</span>' % (m.group(1), html.escape(m.group(2), quote=False))
        pos = m.end()
    return out + html.escape(s[pos:], quote=False)


def P(cmd, comment=""):
    s = "«vio|~/shop» «dim|❯» " + cmd
    if comment:
        s = pad(s, 100) + "«dim|" + comment + "»"
    return s


def code(n, text):
    return "  «dim|%3s │» %s" % (n, text)


def gut(text=""):
    return "  «dim|    │» " + text


TPL = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>%(title)s</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700&amp;display=swap">
<style>
body{margin:0}
.term{box-sizing:border-box;background:#0A0C10;color:#D7DAE0;font-family:'JetBrains Mono',monospace;font-size:13px;border:1px solid #2C313C;border-radius:8px;overflow:hidden;display:flex;flex-direction:column}
.bar{height:32px;flex:none;box-sizing:border-box;background:#1A1D24;border-bottom:1px solid #2C313C;position:relative;display:flex;align-items:center;gap:8px;padding:0 14px}
.dot{width:12px;height:12px;border-radius:50%%}
.btitle{position:absolute;left:0;right:0;text-align:center;color:#8B92A0;font-size:12px;pointer-events:none}
.body{flex:1;min-height:0;overflow:hidden;padding:14px 18px 16px}
.ln{white-space:pre;line-height:20px}
.dim{color:#8B92A0}.ok{color:#4FC27A}.bad{color:#F06A5D}.warn{color:#E5A13A}.info{color:#6AA2F5}.vio{color:#A99BFF}.b{font-weight:700}
.k{color:#8FB8F0}.s{color:#C9D99A}.t{color:#E8B86D}
.cur{background:#D7DAE0;color:#0A0C10}
</style>
</helmet>
<div class="term" style="width: 1280px; height: %(h)dpx">
  <div class="bar"><span class="dot" style="background: #F06A5D"></span><span class="dot" style="background: #E5A13A"></span><span class="dot" style="background: #4FC27A"></span><span class="btitle">zsh — shop</span></div>
  <div class="body">
%(lines)s
  </div>
</div>
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{"$preview":{"width":1280,"height":%(h)d}}'>
class Component extends DCLogic {
  renderVals() {
    return {};
  }
}
</script>
</body>
</html>
"""


def write(fname, title, lines):
    for l in lines:
        if vis(l) > 150:
            raise SystemExit("%s: line too long (%d): %s" % (fname, vis(l), l))
    h = 2 + 32 + 30 + 20 * len(lines) + 8
    body = "\n".join(
        '    <div class="ln">%s</div>' % (render(l) if l.strip() else " ") for l in lines
    )
    with open(os.path.join(OUT, fname), "w") as f:
        f.write(TPL % {"title": title, "h": h, "lines": body})
    print(fname, "lines", len(lines), "height", h, "maxw", max(vis(l) for l in lines))
    return h


HINT = "  «info|hint» "
V = "«vio b|◆ vibranium» «dim|2.0.0»"

# ---------------------------------------------------------------- 1. CliStart
W = [17, 9, 27, 0]
ops = [
    ("create", "POST", "/users", "createUser", "cleanup → users.delete (DELETE on the same resource)"),
    ("get", "GET", "/users/{id}", "getUser", ""),
    ("list", "GET", "/users", "listUsers", "query: limit, status"),
    ("update", "PUT", "/users/{id}", "updateUser", ""),
    ("delete", "DELETE", "/users/{id}", "deleteUser", ""),
    ("startExport", "POST", "/users/exports", "startUserExport", "saves jobId"),
    ("exportStatus", "GET", "/users/exports/{jobId}", "getUserExportStatus", ""),
]
start = [
    P("vc init"),
    "  " + V + "  «dim|·  new project} shop «dim|in ~/shop»".replace("}", "»"),
    "  «ok|created»  " + pad("vibranium.config.yaml", 28) + "«dim|project} shop «dim|· profiles:} default «dim|(active)»".replace("}", "»"),
    "  «ok|created»  " + pad(".env.schema", 28) + "«dim|names the secrets; values go in} .env «dim|(git-ignored)»".replace("}", "»"),
    "  «ok|created»  " + pad(".vscode/settings.json", 28) + "«dim|YAML schemas for the 4 file kinds → completions, hover docs, inline errors»",
    "  «ok|created»  services/  schemas/  datasets/  hooks/  openapi/",
    "  «info|updated»  " + pad(".gitignore", 28) + "«dim|+» .vibranium/  «dim|+» .env",
    HINT + "vc import openapi <url> «dim|to start from a spec, or} vc new api «dim|to write one by hand»".replace("}", "»"),
    "",
    P("vc import openapi https://users.staging.shop.example/v3/api-docs --id users --tests"),
    "  «ok|✓» fetched  https://users.staging.shop.example/v3/api-docs  «dim|JSON · OpenAPI 3.0.3 · 7 operations · 5 schemas · 184ms»",
    "  «ok|wrote»  " + pad("services/users/users.api.yaml", 34) + "«b|7 apis»  «dim|· id} users «dim|· ids from operationId»".replace("}", "»"),
    "         " + row(["«dim|id»", "«dim|method»", "«dim|path»", "«dim|operationId»"], [20, 9, 27, 22]),
]
for i, (oid, m, p, op, note) in enumerate(ops):
    start.append("         " + row(["users.«b|" + oid + "»", m, p, "«dim|" + op + "»", "«dim|" + note + "»" if note else ""], [20, 9, 27, 22]))
start += [
    "  «ok|wrote»  " + pad("openapi/users.yaml", 34) + "«dim|snapshot · the merge base for} vc sync «dim|· commit it}".replace("}", "»"),
    "  «ok|wrote»  " + pad("services/users/users.test.yaml", 34) + "«b|7 tests»  «dim|· id} users.crud «dim|· one happy path per api: status + response schema}".replace("}", "»"),
    "",
    "  «info|note»   base URL → var «b|usersUrl» added to config  «dim|default = https://users.staging.shop.example/api/v1 (servers[0]) · path params → vars»",
    "  «warn|auth»   spec says «b|oauth2 client credentials» → add «b|shopOauth» with  vc new auth shopOauth --kind oauth2-client",
    "         «dim|until then apis send» Authorization: Bearer ${env.USERS_TOKEN} «dim|· added» USERS_TOKEN «dim|to» .env.schema",
    HINT + "vc check «dim|validates everything ·} vc run users «dim|runs the 7 new tests}".replace("}", "»"),
    "",
    P("vc check"),
    "  «ok|✓» «b|3 files · 7 apis · 7 tests · 0 errors»  «dim|· 38ms»",
    "",
    P("vc run users"),
    "  " + V + "  «dim|profile» default  «dim|selector» users  «dim|·  7 tests in 1 file»",
    "",
    "  «ok|✓» «b|users.crud»  «dim|services/users/users.test.yaml»   7/7   1.9s",
]
runs = [
    ("createOk", "POST", "/users", "201", "212ms", ""),
    ("getOk", "GET", "/users/u_93kd41ra", "200", "41ms", "before users.create"),
    ("listOk", "GET", "/users?limit=20", "200", "64ms", ""),
    ("updateOk", "PUT", "/users/u_2mx8c7vd", "200", "118ms", "before users.create"),
    ("deleteOk", "DELETE", "/users/u_q41ne0zt", "204", "57ms", "before users.create"),
    ("startExportOk", "POST", "/users/exports", "202", "96ms", ""),
    ("exportStatusOk", "GET", "/users/exports/ex_41b2", "200", "38ms", "before users.startExport"),
]
for n, m, p, st, t, note in runs:
    start.append("      " + row(["«ok|✓» " + n, "«dim|" + m + "»", "«dim|" + p + "»", "«ok|" + st + "»", pad(" " * (6 - len(t)) + t, 9), "«dim|" + note + "»" if note else ""], [18, 8, 26, 5, 9]))
start += [
    "",
    "  «b|Tests»  «ok b|7 passed» «dim|(7)»   «b|Time» 1.92s «dim|· 14 requests · cleanup users.delete ×4»",
    "  «dim|reports» .vibranium/reports/ «dim|→» ctrf.json · junit.xml · report.html",
    HINT + "vc report --open «dim|to browse requests ·} vc watch users «dim|reruns on save}".replace("}", "»"),
]
h1 = write("CliStart.dc.html", "Vibranium CLI · start from an OpenAPI spec", start)

# ---------------------------------------------------------------- 2. CliAuthor
author = [
    P("vc new api users.deactivate --method POST --path /users/{id}/deactivate"),
    "  «ok|✓» appended api «b|users.deactivate» to services/users/users.api.yaml:84",
    "",
    code(84, "- «k|id»: «s|deactivate»"),
    code(85, "  «k|name»: «s|Deactivate a user»"),
    code(86, "  «k|method»: «s|POST»"),
    code(87, "  «k|path»: «s|/users/${var.id}/deactivate»          «dim|# base URL from the file: ${var.usersUrl}»"),
    code(88, "  «k|vars»:"),
    code(89, "    «k|id»: { «k|type»: «s|string», «k|required»: «t|true» }   «dim|# from the {id} path param»"),
    code(90, "  «k|expect»: { «k|status»: «t|200» }                 «dim|# add body, save or cleanup by hand»"),
    "",
    P("vc new api --id users.rename --from-curl 'curl -X PATCH https://users.staging.shop.example/api/v1/users/u_7f3k29qp"),
    "«dim|quote>» -H \"content-type: application/json\" -d \"{\\\"displayName\\\":\\\"Ada L.\\\",\\\"locale\\\":\\\"en-GB\\\"}\"'",
    "  «dim|parsed»  PATCH /api/v1/users/u_7f3k29qp «dim|· 1 header · JSON body, 2 fields · host matched» ${var.usersUrl} «dim|→ users.api.yaml»",
    "  «ok|✓» appended api «b|users.rename» to services/users/users.api.yaml:93  «dim|· kept content-type, dropped Authorization (file uses} shopOauth«dim|)}".replace("}", "»"),
    "      " + row(["«k|path»", "/users/«s|${var.id}»", "«dim|u_7f3k29qp → var id (required)»"], [9, 36]),
    "      " + row(["«k|body»", "displayName: «s|${var.displayName}»", "«dim|\"Ada L.\" → var displayName (default)»"], [9, 36]),
    "      " + row(["", "locale: «s|${var.locale}»", "«dim|\"en-GB\"  → var locale (default)»"], [9, 36]),
    "",
    P("vc new test users.crud.deactivatesUser --from users.deactivate --negative --auth"),
    "  «ok|✓» wrote «b|4 tests» to services/users/users.test.yaml:112  «dim|· bad-enum: none, users.deactivate has no enum vars»",
    "      " + row(["«b|deactivatesUser»", "«dim|happy path}    200 + body.schema schemas/user.json".replace("}", "»")], [42]),
    "      " + row(["«b|deactivatesUserWithoutIdReturns404»", "«dim|--negative}    missing required var id → 404".replace("}", "»")], [42]),
    "      " + row(["«b|rejectsWrongTypeIdReturns400»", "«dim|--negative}    id = 12345 (number, want string) → 400".replace("}", "»")], [42]),
    "      " + row(["«b|rejectsAnonymous»", "«dim|--auth}        no credentials → 401".replace("}", "»")], [42]),
    code(112, "- «k|id»: «s|deactivatesUser»"),
    code(113, "  «k|name»: «s|Deactivating a user marks it inactive»"),
    code(114, "  «k|before»:"),
    code(115, "    - «k|call»: «s|users.create»                «dim|# its cleanup users.delete runs after the test»"),
    code(116, "      «k|save»: { «k|userId»: «s|${response.body.id}» }"),
    code(117, "  «k|call»: «s|users.deactivate»"),
    code(118, "  «k|vars»: { «k|id»: «s|${var.userId}» }"),
    code(119, "  «k|expect»: { «k|status»: «t|200», «k|body.schema»: «s|schemas/user.json» }"),
    "",
    P("vc call users.deactivate --var id=u_7f3k29qp --profile staging"),
    "  «info|POST» https://users.staging.shop.example/api/v1/users/u_7f3k29qp/deactivate  «ok b|200 OK} «dim|·} 143ms «dim|·} 412 B «dim|· auth} shopOauth «dim|(cached, 52m left)}".replace("}", "»"),
    "",
    "  {",
    "    «k|\"id\"»: «s|\"u_7f3k29qp\"»,",
    "    «k|\"email\"»: «s|\"ada.l@example.test\"»,",
    "    «k|\"displayName\"»: «s|\"Ada L.\"»,",
    "    «k|\"status\"»: «s|\"inactive\"»,",
    "    «k|\"deactivatedAt\"»: «s|\"2026-10-23T09:14:52Z\"»",
    "  }",
    "  «ok|✓» api expect passed  «dim|status 200»",
    HINT + "keep this as a test:  vc call users.deactivate --var id=u_7f3k29qp --save-as-test users.crud.deactivatesKnownUser",
]
h2 = write("CliAuthor.dc.html", "Vibranium CLI · add apis and tests by hand", author)

# ---------------------------------------------------------------- 3. CliRun
inst = [("1/5", "u_93kd41ra", "41ms"), ("2/5", "u_k2p7w0sd", "38ms"), ("3/5", "u_c81hz3mq", "44ms"), ("4/5", "u_0vjr5t2n", "39ms"), ("5/5", "u_6ya3xe9b", "47ms")]
run = [
    P("vc run users.crud.getsCreatedUser"),
    "  " + V + "  «dim|run» #484  «dim|profile» default  «dim|·  1 test, repeat ×5  ·» services/users/users.test.yaml:14",
    "",
    "    " + row(["«ok|✓» «dim|before» ", "users.create ×5", "«ok|5/5»", "«dim|POST /users · avg 158ms»"], [11, 24, 6]),
]
for i, u, t in inst:
    run.append("    " + row(["«ok|✓» getsCreatedUser «dim|[" + i + "]»", "«dim|GET /users/" + u + "»", "«ok|200»", " " * (5 - len(t)) + t], [29, 26, 5]))
run += [
    "    " + row(["«ok|✓» «dim|cleanup»", "users.delete ×5", "«ok|5/5»", "«dim|DELETE /users/{id} · declared on users.create · avg 62ms»"], [11, 24, 6]),
    "",
    "  «ok b|✓ 1 test passed» «dim|(5/5 instances)  ·} 1.21s".replace("}", "»"),
    HINT + "vc watch users.crud.getsCreatedUser «dim|reruns it every time you save the file»",
    "",
    P("vc run -t checkout --profile staging"),
    "  " + V + "  «dim|run» #485  «dim|profile» staging  «dim|tags» checkout",
    "  «dim|plan»  «b|12 tests in 3 files»  «dim|· lifecycle:» beforeRun admin «dim|(cached, 3h left) ·» beforeAll catalog «dim|·» beforeEach ×9",
    "",
    "  " + row(["«ok|✓» «dim|beforeRun»", "admin", "«dim|shopOauth token from cache, expires in 3h 02m»", "  0ms"], [15, 18, 66]),
    "  " + row(["«ok|✓» «dim|beforeAll»", "catalog", "«dim|catalog.createProduct ×3 → saves catalog.productIds»", "412ms"], [15, 18, 66]),
    "  " + row(["«bad|✕» orders.checkout", "«ok|━━━━━━━━━━━━━━»«bad|━━»«warn|━━»«dim|━━»", "10/10", "«ok|7✓» «bad|1✕» «warn|1⊘» «dim|1↷»  «info|(1↻)»", "12.1s"], [21, 22, 8, 22]),
    "  " + row(["«ok|✓» carts.pricing", "«ok|━━━━━━━━━━━━━━━━━━━━»", "1/1", "«ok|1✓»", " 1.4s"], [21, 22, 8, 22]),
    "  " + row(["«ok|✓» catalog.stock", "«ok|━━━━━━━━━━━━━━━━━━━━»", "1/1", "«ok|1✓»", " 0.6s"], [21, 22, 8, 22]),
    "",
    "  «b|orders.checkout»  «dim|services/orders/checkout.test.yaml»",
    "    " + row(["«bad|✕» placesOrder", "«dim|A cart with two items becomes a paid order}".replace("}", "»"), "6.4s"], [26, 82]),
    "    " + row(["«warn|⊘» cancelsPaidOrder", "«dim|blocked: before} payments.authorize «dim|returned} «bad|503} «dim|after 3 tries}".replace("}", "»"), "    –"], [26, 82]),
    "    " + row(["«dim|↷» appliesCoupon", "«dim|skipped: coupon service flaky on staging, see #212 · until Nov 15»", "    –"], [26, 82]),
    "    " + row(["«info|↻» placesGuestOrder", "«dim|passed on retry 2/2 · first try got} «bad|502} «dim|from} payments.authorize".replace("}", "»"), "3.0s"], [26, 82]),
    "    " + row(["«ok|✓» 6 passed", "«dim|rejectsDeclinedCard, rejectsEmptyCart, keepsCartOnFailure, +3»", ""], [26, 82]),
    "",
    "«bad b|error[V5101]»«b|: expectation failed in» orders.checkout.placesOrder «dim|· step» fetch «dim|· GET /orders/ord_8Kq2Tz → 200 after poll ×6»",
    "  «info|  -->» services/orders/checkout.test.yaml:33:11",
    "  «info| 32 │»         «k|match»:",
    "  «info| 33 │»           «k|total»: «s|${var.orderTotal}»",
    "  «info|    │»           «bad b|^^^^^^^^^^^^^^^^^^^^^^^^» «bad|expected} «ok|59.98} «dim|(saved by} orders.place«dim|)}  «bad|actual} «bad b|64.78}".replace("}", "»"),
    "  «info|    =» «b|note»: response has a new field «s|\"tax\"»: «t|4.80» that orderTotal does not include",
    "",
    "«bad b|error[V5102]»«b|: cel check failed} «s|\"total equals sum of lines\"}".replace("}", "»"),
    "  «info|  -->» services/orders/checkout.test.yaml:35:13",
    "  «info| 35 │»           - «k|cel»: «s|body.total == sum(body.items.map(i, i.price * i.quantity))»",
    "  «info|    │»                  «bad|64.78 == 59.98 → false»",
    "",
    "  «b|Tests»    «ok b|9 passed» «dim|·» «bad b|1 failed» «dim|·» «warn b|1 blocked» «dim|·» «b|1 skipped» «dim|·» «info b|1 flaky» «dim|·» «b|14.2s»",
    "  «b|Reports»  .vibranium/reports/ «dim|→» ctrf.json · junit.xml · report.html   «dim|history →» .vibranium/history.db «dim|(run #485)»",
    HINT + "vc run --failed «dim|reruns the 1 failed + 1 blocked ·} vc report --open «dim|shows every request and response}".replace("}", "»"),
]
h3 = write("CliRun.dc.html", "Vibranium CLI · run one, some or all tests", run)

# ---------------------------------------------------------------- 4. CliMaintain
SW = [4, 20, 13]
maint = [
    P("vc sync users"),
    "  «ok|✓» fetched  https://users.staging.shop.example/v3/api-docs  «dim|JSON · OpenAPI 3.0.3 · 8 operations · changed since snapshot of 2026-10-02»",
    "  «dim|3-way merge: base} openapi/users.yaml «dim|· theirs: the spec · yours:} services/users/users.api.yaml".replace("}", "»"),
    "  " + row(["«ok|↓»", "users.list", "«ok|updated»", "spec added query param cursor → var «b|cursor» «dim|(optional)»"], SW),
    "  " + row(["«ok|↓»", "users.get", "«ok|updated»", "spec added query param expand → var «b|expand» «dim|(enum: roles, addresses)»"], SW),
    "  " + row(["«ok|↓»", "users.exportStatus", "«ok|updated»", "path /users/exports/{jobId} → /exports/{jobId}"], SW),
    "  " + row(["«dim|=»", "users.create", "«dim|kept»", "your edit (query var sendWelcome, :9) · spec unchanged"], SW),
    "  " + row(["«ok|+»", "users.restore", "«ok|added»", "POST /users/{id}/restore → services/users/users.api.yaml:121 «dim|· no test yet»"], SW),
    "  " + row(["«warn|−»", "users.legacyImport", "«warn|deprecated»", "removed from spec · kept, marked «k|deprecated»: «t|true» «dim|(2 tests still call it)»"], SW),
    "  " + row(["«bad|✕»", "users.update", "«bad b|conflict»", "both changed since the snapshot · file left as is"], SW),
    "  " + " " * 37 + "«dim|spec}   method  PUT → PATCH".replace("}", "»"),
    "  " + " " * 37 + "«dim|yours»  path    /users/${var.id} → /users/${var.id}/profile   «dim|services/users/users.api.yaml:41»",
    "  " + " " * 37 + "«dim|take the spec:}        vc sync users --take spec users.update".replace("}", "»"),
    "  " + " " * 37 + "«dim|or fix by hand, then:} vc sync users --resolved users.update".replace("}", "»"),
    "",
    "  «b|3 updated · 1 kept · 1 added · 1 deprecated ·» «bad b|1 conflict»   «dim|untouched: users.deactivate, users.rename (not in spec)»",
    "  «dim|snapshot» openapi/users.yaml «dim|updated; users.update stays on the old base until resolved · names, expect, save, cleanup never touched»",
    "",
    P("vc sync --check", "# the same command CI runs on every PR"),
    "  " + row(["«bad|✕»", "orders", "spec changed since snapshot «b|(2 operations)»  «dim|orders.place: new field tax · orders.list: new query var}".replace("}", "»")], [4, 10]),
    "  " + row(["«bad|✕»", "users", "1 unresolved conflict: users.update"], [4, 10]),
    "  " + row(["«ok|✓»", "catalog", "«dim|in sync      –  carts, geo: no} source.openapi «dim|configured}".replace("}", "»")], [4, 10]),
    "  «bad|exit 1»  " + "«dim|·  run} vc sync orders «dim|locally and commit the api file with its snapshot}".replace("}", "»"),
    "",
    P('vc skip orders.checkout.appliesCoupon --reason "coupon service flaky on staging, see #212" --until 2026-11-15'),
    "  «ok|✓» wrote skip to services/orders/checkout.test.yaml:61",
    code(61, "    «k|skip»: { «k|reason»: «s|\"coupon service flaky on staging, see #212\"», «k|until»: «t|2026-11-15» }"),
    "",
    P("vc check"),
    "«warn b|warning[V3104]»«b|: skip expired on 2026-10-20, test runs again»",
    "  «info|  -->» services/catalog/search.test.yaml:47:5  «dim|·} catalog.search.fuzzyQueries".replace("}", "»"),
    "  «info| 47 │»     «k|skip»: { «k|reason»: «s|\"search index rebuild\"», «k|until»: «t|2026-10-20» }",
    "  «info|    =» «b|help»: delete the line, or extend it: vc skip catalog.search.fuzzyQueries --until 2026-11-01",
    "«warn b|warning[V3201]»«b|: DELETE uses shared value ${var.catalog.productIds} from beforeAll»",
    "  «info|  -->» services/catalog/admin.test.yaml:88:18  «dim|·} catalog.admin.deletesProducts".replace("}", "»"),
    "  «info| 87 │»     «k|call»: «s|catalog.deleteProducts»          «dim|# DELETE /products?ids=…»",
    "  «info| 88 │»     «k|vars»: { «k|ids»: «s|${var.catalog.productIds}» }",
    "  «info|    =» «b|help»: beforeAll values are shared by every test in the file; create products in «k|before»: so each test owns its own",
    "  «ok|✓» «b|0 errors · 2 warnings»  «dim|· 58 files · 64 apis · 418 tests · 211ms»",
    "",
    P("vc list tests --skipped"),
    "  " + row(["«dim|test»", "«dim|reason»", "«dim|until»", "«dim|location»"], [38, 46, 20]),
    "  " + row(["orders.checkout.appliesCoupon", "coupon service flaky on staging, see #212", "2026-11-15", "«dim|services/orders/checkout.test.yaml:61»"], [38, 46, 20]),
    "  " + row(["geo.lookup.reverseGeocodeEU", "geocoder sandbox has no EU data", "«dim|no end date»", "«dim|services/geo/lookup.test.yaml:22»"], [38, 46, 20]),
    "  " + row(["catalog.search.fuzzyQueries", "«dim|search index rebuild»", "«warn|expired 10-20»", "«dim|services/catalog/search.test.yaml:47»"], [38, 46, 20]),
    "  «b|2 skipped» «dim|·} «warn|1 expired, runs again} «dim|·  include them in a run with} vc run --include-skipped".replace("}", "»"),
]
h4 = write("CliMaintain.dc.html", "Vibranium CLI · keep in sync and keep tidy", maint)
print("HEIGHTS", h1, h2, h3, h4)
