# Generator for the second batch of CLI artboards (check / new / har) and the TUI import screen.
# Usage: python3 gen_cli2.py <project dir> <check|new|har|tui>
# Shares the markup helpers and template of gen_cli.py (copied, since gen_cli.py writes its own files on import).
import html, re, sys, os

OUT = sys.argv[1]
WHICH = sys.argv[2]
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


# Symbols (arrows, box drawing, checks, bullets) are not in the Google Fonts subsets of JetBrains Mono, so they
# render from a fallback font whose advance width differs; pinning each to one cell (1ch) keeps columns aligned.
SYM = re.compile("([\u2190-\U0001FFFF])")


def esc(t):
    return SYM.sub(r'<span class="g">\1</span>', html.escape(t, quote=False))


def render(s):
    out, pos = "", 0
    for m in MARK.finditer(s):
        out += esc(s[pos:m.start()])
        out += '<span class="%s">%s</span>' % (m.group(1), esc(m.group(2)))
        pos = m.end()
    return out + esc(s[pos:])


def P(cmd, comment=""):
    s = "«vio|~/shop» «dim|❯» " + cmd
    if comment:
        s = pad(s, 100) + "«dim|" + comment + "»"
    return s


def code(n, text):
    return "  «dim|%3s │» %s" % (n, text)


TPL = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "gen_cli.py")).read().split('TPL = """', 1)[1].split('"""', 1)[0]
TPL = TPL.replace(".cur{", ".g{display:inline-block;width:1ch;text-align:center}\n.cur{")


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


# rustc-style diagnostic pieces
def ERR(c, title):
    return "«bad b|error[%s]»«b|: %s»" % (c, title)


def WARN(c, title):
    return "«warn b|warning[%s]»«b|: %s»" % (c, title)


def LOC(where, ctx=""):
    return "  «info|  -->» " + where + ("  «dim|·» " + ctx if ctx else "")


def SRC(n, text):
    return "  «info|%3s │» %s" % (n, text)


def GUT(text=""):
    return "  «info|    │» " + text


def EQ(label, text, cls="b"):
    return "  «info|    =» «%s|%s»: %s" % (cls, label, text)


def FIX(text):
    return "  «info|    =» «ok b|fix»:  " + text


def DCTX(n, text):
    return "     «dim|%3s»  %s" % (n, text)


def DDEL(n, text):
    return "  «bad|-»  «bad|%3s»  «bad|%s»" % (n, text)


def DADD(n, text):
    return "  «ok|+»  «ok|%3s»  «ok|%s»" % (n, text)


def HUNK(rng, ctx, codes):
    return pad("  «info|@@ %s @@»  «dim|%s»" % (rng, ctx), 96) + "«dim|%s»" % codes


def check():
    L = [
        P("vc check"),
        "  " + V + "  «dim|checking» 58 files «dim|· 64 apis · 418 tests · profiles» dev, staging, prod",
        "",
        # V1002
        ERR("V1002", "YAML syntax: `body` sits between two indentation levels"),
        LOC("services/orders/checkout.test.yaml:43:6", "orders.checkout.rejectsDeclinedCard"),
        SRC(41, "    «k|expect»:"),
        SRC(42, "      «k|status»: «t|402»"),
        SRC(43, "     «k|body»:"),
        GUT("     «bad b|^» «bad|column 6: deeper than} «k|expect} «bad|(column 5), not as deep as} «k|status} «bad|(column 7)}".replace("}", "»")),
        SRC(44, "        «k|match»: { «k|error»: «s|card_declined» }"),
        EQ("expected", "a mapping under «k|expect»: every key in it starts at column 7, like «k|status»"),
        EQ("note", "«k|body» is an expect key (status, headers, body, size, timing, cel, custom), so it belongs inside «k|expect»"),
        EQ("help", "indent line 43 by one space; its child lines (44–45) are already deep enough"),
        FIX("«dim|43 │»       «k|body»:"),
        "",
        # V2003
        ERR("V2003", "unknown api `orders.plcae`"),
        LOC("services/orders/checkout.test.yaml:58:15", "orders.checkout.placesGuestOrder «dim|· step» place"),
        SRC(58, "        «k|call»: «s|orders.plcae»"),
        GUT("              «bad b|^^^^^^^^^^^^» «bad|not in} services/orders/orders.api.yaml «dim|(apis: place, get, cancel, refund)}".replace("}", "»")),
        EQ("help", "did you mean «ok b|orders.place»? «dim|(two letters swapped)»"),
        FIX("«dim|58 │»         «k|call»: «s|orders.place»"),
        "",
        # V2101
        ERR("V2101", "${var.couponCode} is used but never set for profiles dev and staging"),
        LOC("services/Orders/CouponTests.test.yaml:14:15", "couponTests.Test_Coupon2"),
        SRC(12, "    «k|call»: «s|orders.place»"),
        SRC(13, "    «k|body»:"),
        SRC(14, "      «k|coupon»: «s|${var.couponCode}»"),
        GUT("              «bad b|^^^^^^^^^^^^^^^^^» «bad|no value under dev or staging; the request would send an empty coupon»"),
        EQ("note", "couponCode could be set in any of these places; only the prod profile sets it:"),
        GUT("  " + row(["«dim|test vars»", "services/Orders/CouponTests.test.yaml:9", "tests › Test_Coupon2 › vars", "«dim|not set»"], [14, 44, 30])),
        GUT("  " + row(["«dim|file vars»", "services/Orders/CouponTests.test.yaml:1", "vars", "«dim|not set»"], [14, 44, 30])),
        GUT("  " + row(["«dim|api vars»", "services/orders/orders.api.yaml:14", "place › vars", "«dim|not set»"], [14, 44, 30])),
        GUT("  " + row(["«dim|profile»", "vibranium.config.yaml:30", "profiles › dev › vars", "«dim|not set»"], [14, 44, 30])),
        GUT("  " + row(["", "vibranium.config.yaml:41", "profiles › staging › vars", "«dim|not set»"], [14, 44, 30])),
        GUT("  " + row(["", "vibranium.config.yaml:52", "profiles › prod › vars", "«ok|✓» «k|couponCode»: «s|WELCOME10»"], [14, 44, 30])),
        EQ("help", "same code everywhere? set it once in the test file: «k|vars»: { «k|couponCode»: «s|WELCOME10» }"),
        EQ("help", "differs per environment? add it under profiles › staging › vars and profiles › dev › vars in vibranium.config.yaml"),
        EQ("help", "or give this use a fallback: «s|${var.couponCode | default('WELCOME10')}»"),
        "",
        # V2302
        ERR("V2302", "type: `retry.attempts` must be an integer, got the string \"3\""),
        LOC("services/orders/history.test.yaml:27:17", "orders.history.listsPastOrders"),
        SRC(26, "    «k|retry»:"),
        SRC(27, "      «k|attempts»: «s|\"3\"»"),
        GUT("                «bad b|^^^» «bad|quoted, so YAML reads it as text; attempts takes a whole number from 1 to 10»"),
        EQ("help", "remove the quotes; to take it from a var, convert it: «k|attempts»: «s|${var.retries | int}»"),
        FIX("«dim|27 │»       «k|attempts»: «t|3»"),
        "",
        # V2205
        ERR("V2205", "cycle: flows.verifiedMember reuses itself through users.crud.signsUp"),
        LOC("services/shared/flows.test.yaml:12:15", "flows.verifiedMember «dim|· step» signUp"),
        GUT(),
        GUT("  " + row(["«bad|┌─▶» «b|flows.verifiedMember»", "services/shared/flows.test.yaml:12", "«dim|step signUp»", "«k|test»: «s|users.crud.signsUp»"], [28, 38, 16])),
        GUT("  " + "«bad|│»       «bad|│»"),
        GUT("  " + "«bad|│»       «bad|▼»"),
        GUT("  " + row(["«bad|│»   «b|users.crud.signsUp»", "services/users/users.test.yaml:64", "«dim|before»", "«k|test»: «s|flows.verifiedMember»  «bad|← back to the start»"], [28, 38, 16])),
        GUT("  " + "«bad|│»       «bad|│»"),
        GUT("  " + "«bad|└───────┘»"),
        GUT(),
        EQ("why", "«k|test»: runs the whole reused test, its before and steps included, so these two would run each other forever"),
        EQ("help", "signsUp only needs a member to exist: at users.test.yaml:64 replace «k|test»: «s|flows.verifiedMember» with «k|call»: «s|users.create»"),
        EQ("help", "vc explain V2205 «dim|has a worked example · in the TUI, Explore → ctrl-g draws the reuse graph»"),
        "",
        # V4001
        WARN("V4001", "naming: test id `Test_Coupon2` does not describe a behaviour"),
        LOC("services/Orders/CouponTests.test.yaml:9:9"),
        SRC(9, "  - «k|id»: «s|Test_Coupon2»"),
        GUT("        «warn b|^^^^^^^^^^^^» «warn|underscore, capital first letter, a number instead of a behaviour»"),
        SRC(10, "    «k|name»: «s|Applies a coupon twice»"),
        GUT("          «info|----------------------» «info|id from the name →} «ok b|appliesCouponTwice}".replace("}", "»")),
        EQ("rule", "test ids are camelCase, a behaviour in the third person with the subject implied: createsMember, rejectsBadEmail"),
        EQ("help", "rename it to «ok b|appliesCouponTwice»"),
        FIX("renames the id and its 1 reference «dim|·» CouponTests.test.yaml:31  «k|test»: «s|couponTests.Test_Coupon2»"),
        "",
        # V4003
        WARN("V4003", "naming: test file `services/Orders/CouponTests.test.yaml` is off the folder convention"),
        LOC("services/Orders/CouponTests.test.yaml:1:5"),
        SRC(1, "«k|id»: «s|couponTests»"),
        GUT("    «warn b|^^^^^^^^^^^» «warn|conventional:} «ok b|orders.coupons} «warn|at} «ok b|services/orders/coupons.test.yaml}".replace("}", "»")),
        EQ("rule", "test files live at services/<service>/<area>.test.yaml, lowercase, with id <service>.<area>"),
        EQ("note", "services/Orders/ and services/orders/ differ only in case: on macOS and Windows they are the same folder"),
        EQ("help", "vc mv services/Orders/CouponTests.test.yaml services/orders/coupons.test.yaml"),
        GUT("      «dim|moves it with git mv, sets} «k|id»: «s|orders.coupons}«dim|, rewrites} couponTests.* → orders.coupons.* «dim|(3 references, 2 files)}".replace("}", "»")),
        EQ("note", "not part of --fix: a move is best in its own commit, so review shows it as a rename"),
        "",
        # V3104
        WARN("V3104", "skip expired on 2026-10-20, test runs again"),
        LOC("services/catalog/search.test.yaml:47:5", "catalog.search.fuzzyQueries"),
        SRC(47, "    «k|skip»: { «k|reason»: «s|\"search index rebuild\"», «k|until»: «t|2026-10-20» }"),
        EQ("help", "delete the line, or extend it: vc skip catalog.search.fuzzyQueries --until 2026-11-01"),
        "",
        pad("  «bad b|✕ 5 errors» «dim|·» «warn b|3 warnings» «dim|·» «ok b|4 fixable» «dim|· run» vc check --fix «dim|to preview»", 96)
        + "«dim|58 files · 64 apis · 418 tests · 3 profiles · 214ms»",
        "  «bad|exit 1»",
        "",
        # --fix
        P("vc check --fix"),
        "  «b|4 fixes in 3 files»  «dim|· preview, nothing written yet}".replace("}", "»"),
        "",
        "  «b|services/orders/checkout.test.yaml»  «dim|2 fixes»",
        HUNK("-41,5 +41,5", "orders.checkout.rejectsDeclinedCard", "V1002 indentation"),
        DCTX(41, "    expect:"),
        DCTX(42, "      status: 402"),
        DDEL(43, "     body:"),
        DADD(43, "      body:"),
        DCTX(44, "        match: { error: card_declined }"),
        HUNK("-58 +58", "orders.checkout.placesGuestOrder", "V2003 unknown api"),
        DDEL(58, "        call: orders.plcae"),
        DADD(58, "        call: orders.place"),
        "",
        "  «b|services/orders/history.test.yaml»  «dim|1 fix»",
        HUNK("-26,2 +26,2", "orders.history.listsPastOrders", "V2302 type"),
        DCTX(26, "    retry:"),
        DDEL(27, "      attempts: \"3\""),
        DADD(27, "      attempts: 3"),
        "",
        "  «b|services/Orders/CouponTests.test.yaml»  «dim|1 fix, 2 lines»",
        HUNK("-9 +9", "couponTests.Test_Coupon2", "V4001 naming"),
        DDEL(9, "  - id: Test_Coupon2"),
        DADD(9, "  - id: appliesCouponTwice"),
        HUNK("-31 +31", "couponTests.stacksWithSalePrice · step coupon", "V4001 reference"),
        DDEL(31, "        test: couponTests.Test_Coupon2"),
        DADD(31, "        test: couponTests.appliesCouponTwice"),
        "",
        "  «vio b|?» «b|Apply 4 fixes?» «dim|[Y/n]» y",
        "  «ok|✓» «b|applied 4 fixes in 3 files» «dim|·» «bad b|2 errors need you»",
        "      " + row(["«bad|✕» V2101", "${var.couponCode} is not set for dev, staging", "«dim|services/Orders/CouponTests.test.yaml:14»"], [12, 56]),
        "      " + row(["«bad|✕» V2205", "cycle flows.verifiedMember ⇄ users.crud.signsUp", "«dim|services/shared/flows.test.yaml:12»"], [12, 56]),
        "      " + row(["«warn|!» V4003", "file off convention, move with vc mv", "«dim|services/Orders/CouponTests.test.yaml»"], [12, 56]),
        "      " + row(["«warn|!» V3104", "expired skip", "«dim|services/catalog/search.test.yaml:47»"], [12, 56]),
        HINT + "git diff «dim|shows exactly what changed; nothing is staged or committed»",
        "",
        # explain
        P("vc explain V2205"),
        "  «bad b|V2205»  «b|error · cycle in reused tests»",
        "",
        "  A step or lifecycle action written as «k|test»: «s|<testId>» runs that whole test inside this one: its before, steps and after.",
        "  If test A reuses B, and B reuses A (directly or through other tests), the run could never finish. vc check reports it,",
        "  vc run refuses to start, and nothing is sent. Lifecycle actions and steps that use «k|call»: are never part of a cycle.",
        "",
        "  " + row(["«bad b|✕ fails»", "«ok b|✓ works»  «dim|break the loop where only data is needed»"], [64]),
        "  " + row(["  «dim|# services/shared/flows.test.yaml»", "  «dim|# services/users/users.test.yaml»"], [64]),
        "  " + row(["  - «k|id»: «s|verifiedMember»", "  - «k|id»: «s|signsUp»"], [64]),
        "  " + row(["    «k|steps»:", "    «k|before»:"], [64]),
        "  " + row(["      - { «k|id»: «s|signUp», «k|test»: «s|users.crud.signsUp» }", "      - «k|call»: «s|users.create»       «dim|# the member it needs»"], [64]),
        "  " + row(["      - { «k|call»: «s|users.update», «k|vars»: { «k|emailVerified»: «t|true» } }", "    «k|call»: «s|users.get»"], [64]),
        "  " + row(["  «dim|# services/users/users.test.yaml»", "    «k|expect»: { «k|status»: «t|200» }"], [64]),
        "  " + row(["  - «k|id»: «s|signsUp»", ""], [64]),
        "  " + row(["    «k|before»:", "  «dim|# flows.test.yaml stays as it is: reusing»"], [64]),
        "  " + row(["      - «k|test»: «s|flows.verifiedMember»   «bad|# ← cycle»", "  «dim|# users.crud.signsUp is fine once it no»"], [64]),
        "  " + row(["    «k|call»: «s|users.get»", "  «dim|# longer reuses verifiedMember back»"], [64]),
        "",
        "  «dim|see also}  vc explain V2003 «dim|unknown api ·} vc explain --list «dim|every code with a one-line summary}".replace("}", "»"),
    ]
    return write("CliCheck.dc.html", "Vibranium CLI · errors that tell you how to fix them", L)


def Q(question, answer, typing=False, note=""):
    s = "«vio b|?» «b|%s» «dim|›» «info b|%s»" % (question, answer)
    if typing:
        s += "«cur| »"
    if note:
        s += "  " + note
    return s


def CH(sel, cells, widths):
    if sel:
        return "  «vio b|❯» " + row(["«vio b|" + cells[0] + "»"] + cells[1:], widths)
    return "    " + row(["«dim|" + cells[0] + "»"] + ["«dim|" + c + "»" if c else "" for c in cells[1:]], widths)


def new():
    SVC = [17, 21, 16]
    FILE = [18, 38, 10]
    L = [
        P("vc new test"),
        "  " + V + "  «dim|new test · ↑↓ move · enter choose · each answer is also a flag, see} vc new test --help".replace("}", "»"),
        "",
        Q("What should the test show? Write it as a sentence", "Rejects a declined card", True),
        "    «dim|id}  «ok b|rejectsDeclinedCard}   «dim|updates as you type: the verb comes first, camelCase, a / an / the dropped}   «ok|✓ valid}".replace("}", "»"),
        "",
        Q("Which service?", "orders", note="«dim|type to filter»"),
        CH(False, ["users", "services/users/", "2 test files", "9 apis"], SVC),
        CH(True, ["orders", "services/orders/", "2 test files", "«dim|4 apis»"], SVC),
        CH(False, ["catalog", "services/catalog/", "3 test files", "3 apis"], SVC),
        CH(False, ["carts", "services/carts/", "1 test file", "3 apis"], SVC),
        CH(False, ["geo", "services/geo/", "1 test file", "3 apis"], SVC),
        CH(False, ["payments", "services/payments/", "no tests yet", "1 api"], SVC),
        "",
        Q("Which file?", "+ new file"),
        "    " + row(["«dim|orders.checkout»", "«dim|services/orders/checkout.test.yaml»", "«dim|10 tests»", "«warn|has rejectsDeclinedCard already (:38), ids are unique per file»"], FILE),
        CH(False, ["orders.history", "services/orders/history.test.yaml", "4 tests", ""], FILE),
        CH(True, ["+ new file", "services/orders/«dim|<area>».test.yaml", "", "«dim|one area of the service per file»"], FILE),
        Q("Area, one lowercase word", "payments", True),
        "    «dim|file}  services/orders/«ok b|payments}.test.yaml   «dim|id}  orders.«ok b|payments}   «ok|✓ follows services/<service>/<area>.test.yaml · folder exists}".replace("}", "»"),
        "",
        Q("Start from which api?", "ord plac", True, "«dim|fuzzy, over 64 apis · 1 match»"),
        "  «vio b|❯} «vio b|ord}«vio|ers.}«vio b|plac}«vio|e}    «info|POST}  /orders     «dim|vars} cartId «dim|(required),} paymentToken «dim|· spec responses} 201 400 401 402".replace("}", "»"),
        Q("Expected status", "402", note="«dim|Payment Required · from openapi/orders.yaml»"),
        "",
        Q("Also generate", "3 selected", note="«dim|space toggles · enter confirms»"),
        "    " + row(["«vio b|◉» negative cases", "«dim|cartId of the wrong type → 400»", "«dim|+1 test»"], [22, 46]),
        "    " + row(["«vio b|◉» 401", "«dim|the same request without credentials»", "«dim|+1 test»"], [22, 46]),
        "    " + row(["«dim|○ response schema»", "«dim|body.schema from schemas/orders/order.json»", ""], [22, 46]),
        "    " + row(["«vio b|◉» setup", "«dim|copy beforeEach from orders.checkout»", "«dim|orders.place needs a cart»"], [22, 46]),
        "",
        "  «ok|✓» wrote «b|services/orders/payments.test.yaml}  «dim|· new file · id} orders.payments «dim|· 3 tests}".replace("}", "»"),
        code(1, "«k|id»: «s|orders.payments»"),
        code(2, "«k|name»: «s|Order payments»"),
        code(3, "«k|tags»: [«s|orders», «s|payments»]"),
        code(4, "«k|beforeEach»:                                 «dim|# orders.place needs a cart: same setup as orders.checkout»"),
        code(5, "  - «k|call»: «s|users.create»"),
        code(6, "  - «k|call»: «s|carts.create»"),
        code(7, "  - «k|call»: «s|carts.addItem»"),
        code(8, "«k|tests»:"),
        code(9, "  - «k|id»: «s|rejectsDeclinedCard»"),
        code(10, "    «k|name»: «s|Rejects a declined card»"),
        code(11, "    «k|call»: «s|orders.place»"),
        code(12, "    «k|vars»: { «k|paymentToken»: «s|tok_declined» }          «dim|# a token your payments sandbox always declines»"),
        code(13, "    «k|expect»: { «k|status»: «t|402» }"),
        code(14, "  - «k|id»: «s|rejectsNumericCartId»"),
        code(15, "    «k|name»: «s|Rejects a numeric cart id»"),
        code(16, "    «k|call»: «s|orders.place»"),
        code(17, "    «k|vars»: { «k|cartId»: «t|12345» }                       «dim|# the spec says cartId is a string»"),
        code(18, "    «k|expect»: { «k|status»: «t|400» }"),
        code(19, "  - «k|id»: «s|rejectsAnonymousOrder»"),
        code(20, "    «k|name»: «s|Rejects an anonymous order»"),
        code(21, "    «k|call»: «s|orders.place»"),
        code(22, "    «k|auth»: «s|none»"),
        code(23, "    «k|expect»: { «k|status»: «t|401» }"),
        HINT + "vc run orders.payments «dim|· the same in one line:} vc new test \"Rejects a declined card\" --file orders.payments --from orders.place --status 402".replace("}", "»"),
        "",
        P("vc new api orders.Get-Refunds"),
        ERR("V4002", "invalid api id `Get-Refunds`"),
        EQ("rule", "api ids are camelCase and start with a verb; no dashes, no service prefix: the file id «b|orders» supplies it"),
        EQ("help", "did you mean"),
        GUT("  " + row(["«ok b|orders.listRefunds»", "«dim|list» for a collection   GET /orders/${var.orderId}/refunds", "«dim|← preferred»"], [24, 60])),
        GUT("  " + row(["«ok|orders.getRefunds»", "«dim|also valid, same request; get is usually kept for one item»", ""], [24, 60])),
        EQ("note", "it goes in services/orders/orders.api.yaml «dim|(file id} orders«dim|), next to} orders.refund".replace("}", "»")),
        EQ("help", "vc new api orders.listRefunds --method GET --path /orders/{orderId}/refunds"),
        "  «bad|exit 2}  «dim|· nothing written}".replace("}", "»"),
        "",
        P("vc rename users.crud.Test1 users.crud.createsMember"),
        "  «dim|test} users.crud.Test1 «dim|→} «ok b|users.crud.createsMember}   «dim|its name \"Creates a member\" gives the same id ·} «ok|clears V4001}".replace("}", "»"),
        "  «ok|✓» «b|updated 6 references in 4 files»",
        "    " + row(["services/users/users.test.yaml:22", "«k|id»: «bad|Test1»", "→ «ok|createsMember»", "«dim|the definition»"], [40, 34, 34]),
        "    " + row(["services/users/users.test.yaml:58", "«k|test»: «bad|users.crud.Test1»", "→ «ok|users.crud.createsMember»", "«dim|step in deletesMember»"], [40, 34, 34]),
        "    " + row(["services/users/reports.test.yaml:9", "«k|test»: «bad|users.crud.Test1»", "→ «ok|users.crud.createsMember»", "«dim|beforeAll»"], [40, 34, 34]),
        "    " + row(["services/shared/flows.test.yaml:14", "«k|test»: «bad|users.crud.Test1»", "→ «ok|users.crud.createsMember»", "«dim|step in verifiedMember»"], [40, 34, 34]),
        "    " + row(["services/shared/flows.test.yaml:31", "«k|test»: «bad|users.crud.Test1»", "→ «ok|users.crud.createsMember»", "«dim|step in adminMember»"], [40, 34, 34]),
        "    " + row(["load/checkout.load.yaml:12", "«k|test»: «bad|users.crud.Test1»", "→ «ok|users.crud.createsMember»", "«dim|mix, weight 1»"], [40, 34, 34]),
        "  «dim|history} .vibranium/history.db «dim|keeps past runs of Test1 under the new id · files changed, nothing staged}".replace("}", "»"),
        "",
        P("vc new file test orders.coupons"),
        "  «ok|✓» created «b|services/orders/coupons.test.yaml}  «dim|· id} orders.coupons «dim|· path from the id: services/<service>/<area>.test.yaml}".replace("}", "»"),
        code(1, "«k|id»: «s|orders.coupons»"),
        code(2, "«k|name»: «s|Coupons»                        «dim|# a short title; each test's name is a sentence»"),
        code(3, "«k|tags»: [«s|orders», «s|coupons»]              «dim|# lowercase, kebab-case for two words»"),
        code(4, "«dim|# vars:         values every test here can use, e.g. couponCode: WELCOME10»"),
        code(5, "«dim|# beforeEach:   actions before each test; orders.place needs a cart:»"),
        code(6, "«dim|#   - call: carts.create»"),
        code(7, "«k|tests»: []                            «dim|# add one: vc new test --file orders.coupons»"),
        "",
        "  «dim|────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────»",
        "  «dim|conventions applied»",
        "  " + row(["«dim|test id»", "«dim|camelCase, verb first, from the name}".replace("}", "»"), "«dim|Rejects a declined card → rejectsDeclinedCard»"], [14, 50]),
        "  " + row(["«dim|test file»", "«dim|services/<service>/<area>.test.yaml}".replace("}", "»"), "«dim|id <service>.<area> → orders.payments, orders.coupons»"], [14, 50]),
        "  " + row(["«dim|api id»", "«dim|camelCase verb + thing, no service prefix»", "«dim|list for collections, get for one → orders.listRefunds»"], [14, 50]),
        "  " + row(["«dim|renames»", "«dim|every reference updated, history kept»", "«dim|vc rename · vc check --fix for V4001 test ids»"], [14, 50]),
    ]
    return write("CliNew.dc.html", "Vibranium CLI · creating things the conventional way", L)


if WHICH == "check":
    check()
if WHICH == "new":
    new()


def har():
    AW = [6, 8, 34, 22]
    HW = [33, 20, 39]
    L = [
        P("vc import har ~/Downloads/checkout-session.har --flow orders.checkout.recordedCheckout"),
        "  " + V + "  «dim|import har · profile» staging",
        "  «ok|✓» read  ~/Downloads/checkout-session.har  «dim|· HAR 1.2 ·» «b|46 entries» «dim|· recorded 2026-10-22 16:41–16:43 · 3.8 MB»",
        "",
        "  «b|filter»",
        "    " + row(["«dim|−» 21", "static assets", "«dim|.js .css .woff2 .png .svg from shop.example, cdn.shop.example»"], [8, 22]),
        "    " + row(["«dim|−» 6", "third-party", "«dim|analytics.example.net ×4 · chat-widget.example.com ×2»"], [8, 22]),
        "    " + row(["«dim|−» 5", "OPTIONS", "«dim|CORS preflights the browser sent before 5 of the POSTs»"], [8, 22]),
        "    " + row(["«ok b|=» «b|14»", "«b|requests»", "«dim|to 4 api hosts ·} --dry-run «dim|lists every dropped entry ·} --include-host «dim|keeps one}".replace("}", "»")], [8, 22]),
        "",
        "  «b|host → api file»",
        "    " + row(["users.staging.shop.example", "«s|${var.usersUrl}»", "services/users/users.api.yaml", "«dim|existing · id users · 9 apis»"], HW),
        "    " + row(["carts.staging.shop.example", "«s|${var.cartsUrl}»", "services/carts/carts.api.yaml", "«ok|new file» «dim|· id carts · var cartsUrl added»"], HW),
        "    " + row(["orders.staging.shop.example", "«s|${var.ordersUrl}»", "services/orders/orders.api.yaml", "«dim|existing · id orders · 4 apis»"], HW),
        "    " + row(["payments.staging.shop.example", "«s|${var.paymentsUrl}»", "services/payments/payments.api.yaml", "«ok|new file» «dim|· id payments · var paymentsUrl added»"], HW),
        "    «dim|new vars: staging = the recorded host; dev and prod follow the usersUrl pattern (carts.dev.shop.example, carts.shop.example)»",
        "",
        "  «b|apis}  «dim|14 requests → 9 apis · ids follow <file id>.<verb><Thing> · paths are relative to the file's baseUrl}".replace("}", "»"),
        "    " + row(["«dim|entry»", "«dim|method»", "«dim|path»", "«dim|id»", "«dim|action»"], AW),
        "    " + row(["07", "GET", "/users/«s|${var.userId}»", "users.get", "«dim|= matches existing  users.api.yaml:31»"], AW),
        "    " + row(["08", "GET", "/users/«s|${var.userId}»/addresses", "users.«b|listAddresses»", "«ok|+ new»"], AW),
        "    " + row(["12", "POST", "/carts", "carts.«b|create»", "«ok|+ new»"], AW),
        "    " + row(["15", "POST", "/carts/«s|${var.cartId}»/items", "carts.«b|addItem»", "«ok|+ new» «dim|·} «info|merged ×3} «dim|· entries 15, 18, 21 differ only in} «k|sku}".replace("}", "»")], AW),
        "    " + row(["22", "GET", "/carts/«s|${var.cartId}»", "carts.«b|get»", "«ok|+ new» «dim|·} «info|merged ×2} «dim|· entries 22, 30}".replace("}", "»")], AW),
        "    " + row(["27", "GET", "/payment-methods", "payments.«b|listMethods»", "«ok|+ new»"], AW),
        "    " + row(["32", "POST", "/authorizations", "payments.«b|authorize»", "«ok|+ new» «dim|· query} «k|api_key} «dim|is a secret, see below}".replace("}", "»")], AW),
        "    " + row(["35", "POST", "/orders", "orders.place", "«dim|= matches existing  orders.api.yaml:9 · same body keys»"], AW),
        "    " + row(["38", "GET", "/orders/«s|${var.orderId}»", "orders.get", "«dim|= matches existing ·} «info|merged ×3} «dim|· polled until} «k|status}«dim|:} «s|paid}".replace("}", "»")], AW),
        "    «dim|path ids:} u_7f3k29qp → «k|userId}  «dim|·} c_4Hq9Lm → «k|cartId}  «dim|·} ord_9Tk2Px → «k|orderId}   «dim|ids that vary between entries became path vars}".replace("}", "»"),
        "",
        "  «b|chains}  «dim|a value from one response that reappears in a later request becomes a save, and later steps use the var}".replace("}", "»"),
        "    " + row(["c_4Hq9Lm", "carts.create → body.id", "used by 6 later requests", "→ save «k|cartId»"], [14, 38, 34]),
        "    " + row(["42.50", "carts.get → body.total", "payments.authorize body.amount", "→ save «k|cartTotal»"], [14, 38, 34]),
        "    " + row(["pt_7Qm2Xc", "payments.authorize → body.token", "orders.place body.paymentToken", "→ save «k|paymentToken»"], [14, 38, 34]),
        "    " + row(["ord_9Tk2Px", "orders.place → body.id", "orders.get path ×3", "→ save «k|orderId»"], [14, 38, 34]),
        "    " + row(["u_7f3k29qp", "«dim|in no response: the signed-in user»", "", "→ var «k|userId» «dim|(default u_7f3k29qp)»"], [14, 38, 34]),
        "",
        "  «b|secrets}  «dim|found in the recording and removed; none is written to disk}".replace("}", "»"),
        "    " + row(["«warn|Authorization: Bearer eyJhbGciOi…»", "12 requests", "→ removed; the files use auth «b|shopOauth» «dim|(same token issuer)»"], [38, 14]),
        "    " + row(["«warn|Cookie: session=…»", "14 requests", "→ dropped; apis authenticate with shopOauth, not the browser session"], [38, 14]),
        "    " + row(["«warn|?api_key=…» «dim|(32 chars)»", "1 request", "→ «s|${secret.PAYMENTS_KEY}» «dim|·» PAYMENTS_KEY «dim|added to» .env.schema«dim|; set its value in» .env"], [38, 14]),
        "    «warn|!} «dim|the .har file still holds all three: delete it, or keep it out of the repo}".replace("}", "»"),
        "",
        "  «b|body → vars}  «dim|recorded values become defaults, so each api runs as recorded and every value can be overridden}".replace("}", "»"),
        "    " + row(["carts.addItem", "«k|sku» «s|\"SKU-1042\"» · «k|quantity» «t|1»", "«dim|vars sku, quantity · the 3 recorded skus → var} «k|skus} «dim|in the flow}".replace("}", "»")], [22, 40]),
        "    " + row(["payments.authorize", "«k|amount» «t|42.50» · «k|currency» «s|\"EUR\"»", "«dim|vars amount, currency · amount chained from} «k|cartTotal}".replace("}", "»")], [22, 40]),
        "    " + row(["orders.place", "«k|cartId» · «k|paymentToken»", "«dim|already vars in orders.api.yaml · both chained»"], [22, 40]),
        "",
        "  «ok|✓» wrote test «b|orders.checkout.recordedCheckout}  «dim|services/orders/checkout.test.yaml:142 · 6 steps}".replace("}", "»"),
        code(142, "  - «k|id»: «s|recordedCheckout»"),
        code(143, "    «k|name»: «s|Recorded checkout, from checkout-session.har»"),
        code(144, "    «k|standalone»: «t|true»                «dim|# skips this file's beforeEach: the recording makes its own cart»"),
        code(145, "    «k|vars»: { «k|skus»: [«s|SKU-1042», «s|SKU-2210», «s|SKU-0087»] }"),
        code(146, "    «k|steps»:"),
        code(147, "      - { «k|id»: «s|cart»,      «k|call»: «s|carts.create»,       «k|save»: { «k|cartId»: «s|${response.body.id}» } }"),
        code(148, "      - { «k|id»: «s|items»,     «k|call»: «s|carts.addItem»,      «k|forEach»: «s|${var.skus}», «k|vars»: { «k|sku»: «s|${item}» } }"),
        code(149, "      - { «k|id»: «s|review»,    «k|call»: «s|carts.get»,          «k|save»: { «k|cartTotal»: «s|${response.body.total}» } }"),
        code(150, "      - { «k|id»: «s|authorize», «k|call»: «s|payments.authorize», «k|vars»: { «k|amount»: «s|${var.cartTotal}» }, «k|save»: { «k|paymentToken»: «s|${response.body.token}» } }"),
        code(151, "      - { «k|id»: «s|place»,     «k|call»: «s|orders.place»,       «k|save»: { «k|orderId»: «s|${response.body.id}» } }"),
        code(152, "      - { «k|id»: «s|fetch»,     «k|call»: «s|orders.get»,         «k|poll»: { «k|until»: { «k|body»: { «k|match»: { «k|status»: «s|paid» } } }, «k|every»: «t|1s», «k|timeout»: «t|30s» } }"),
        code(153, "    «k|expect»: { «k|status»: «t|200», «k|body»: { «k|match»: { «k|status»: «s|paid» } } }   «dim|# from the last recorded response»"),
        "",
        "  «b|files»",
        "    " + row(["«ok|+»", "services/carts/carts.api.yaml", "«dim|new · 3 apis: create, addItem, get»"], [3, 42]),
        "    " + row(["«ok|+»", "services/payments/payments.api.yaml", "«dim|new · 2 apis: listMethods, authorize»"], [3, 42]),
        "    " + row(["«info|~»", "services/users/users.api.yaml", "«dim|+1 api: listAddresses · users.get unchanged»"], [3, 42]),
        "    " + row(["«info|~»", "services/orders/checkout.test.yaml", "«dim|+1 test: recordedCheckout, 6 steps»"], [3, 42]),
        "    " + row(["«info|~»", "vibranium.config.yaml", "«dim|+2 vars cartsUrl, paymentsUrl in dev, staging, prod»"], [3, 42]),
        "    " + row(["«info|~»", ".env.schema", "«dim|+PAYMENTS_KEY»"], [3, 42]),
        "    " + row(["«dim|=»", "«dim|services/orders/orders.api.yaml»", "«dim|unchanged: place and get already match the recording»"], [3, 42]),
        "",
        P("vc check"),
        "  «ok|✓» «b|0 errors · 0 warnings»  «dim|· 60 files · 70 apis · 419 tests · secrets 4 of 4 set · 226ms»",
        "",
        P("vc run orders.checkout.recordedCheckout --profile staging"),
        "  " + V + "  «dim|run» #486  «dim|profile» staging  «dim|· 1 test · standalone ·» services/orders/checkout.test.yaml:142",
        "",
    ]
    RW = [16, 7, 26, 6, 10]
    for name, m, p, st, t, note in [
        ("cart", "POST", "/carts", "201", "118ms", "saved «k|cartId» c_8Rw2Mn"),
        ("items «dim|×3»", "POST", "/carts/c_8Rw2Mn/items", "201", " 96ms", "«dim|avg ·» SKU-1042, SKU-2210, SKU-0087"),
        ("review", "GET", "/carts/c_8Rw2Mn", "200", " 44ms", "saved «k|cartTotal» 42.50"),
        ("authorize", "POST", "/authorizations", "201", "310ms", "saved «k|paymentToken» pt_3Ld8Wv  «dim|· api_key from» «s|${secret.PAYMENTS_KEY}»"),
        ("place", "POST", "/orders", "201", "296ms", "saved «k|orderId» ord_5Vb8Qe"),
        ("fetch", "GET", "/orders/ord_5Vb8Qe", "200", " 3.1s", "poll ×4 until «k|status»: «s|paid»"),
    ]:
        L.append("    " + row(["«ok|✓» " + name, "«dim|" + m + "»", "«dim|" + p + "»", "«ok|" + st + "»", t, note], RW))
    L += [
        "",
        "  «ok b|✓ 1 test passed}  «dim|· 6 steps · 11 requests ·} 4.2s".replace("}", "»"),
        HINT + "vc import har <file> --dry-run «dim|prints this plan without writing ·} --include-host <host> «dim|keeps a host the filter dropped}".replace("}", "»"),
    ]
    return write("CliHar.dc.html", "Vibranium CLI · import a HAR and run it", L)


if WHICH == "har":
    har()


def start():
    # vc init (config, folders, annotated sample against vc demo) → run the sample → import a spec → check → run
    F = 34  # file column after "  created  "
    NX = [41]  # next-steps command column, lands on the same column as the file descriptions
    L = [
        P("vc init"),
        "  " + V + "  «dim|·  new project} shop «dim|in ~/shop»".replace("}", "»"),
        "  «ok|created»  " + pad("vibranium.config.yaml", F)
        + "«dim|defaults for} timeout, concurrency, headers, reports, history  «dim|· profile} default «dim|(active)}".replace("}", "»"),
        "  «ok|created»  " + pad(".env.schema", F) + "«dim|names the secrets; values go in} .env «dim|(git-ignored)}".replace("}", "»"),
        "  «ok|created»  " + pad(".gitignore", F) + "«dim|+» .vibranium/  «dim|+» .env",
        "  «ok|created»  schemas/  datasets/  hooks/  openapi/  load/",
        "",
        "  «vio b|sample»   «dim|annotated starter that runs offline against the built-in demo service} vc demo «dim|on} localhost:7421".replace("}", "»"),
        "  «ok|created»  " + pad("services/sample/sample.api.yaml", F)
        + "«b|4 apis}   «dim|· id} sample «dim|·} listUsers, createUser, getUser, deleteUser".replace("}", "»"),
        "  «ok|created»  " + pad("services/sample/basics.test.yaml", F)
        + "«b|4 tests}  «dim|· id} sample.basics «dim|· starts} vc demo «dim|before the tests and stops it after}".replace("}", "»"),
        "",
        "  «dim|every file starts with a schema line for editor autocomplete and a short doc header with links to the docs»",
        "  «dim|comments in generated files} --comments full|brief|none  «dim|·  skip the sample} --no-sample".replace("}", "»"),
        "",
        "  «b|next»",
        "    " + row(["vc run sample.basics", "«ok|4 passed}«dim|, offline: no server, keys or config needed}".replace("}", "»")], NX),
        "    " + row(["vc import openapi <url>", "«dim|add your own service from its spec»"], NX),
        "    " + row(["vc new test …", "«dim|write a test step by step; each answer is also a flag»"], NX),
        "",
        P("vc run sample.basics"),
        "  " + V + "  «dim|profile» default  «dim|selector» sample.basics  «dim|·  4 tests in 1 file ·» vc demo «dim|on» localhost:7421",
        "  «ok|✓» «b|sample.basics»  «dim|services/sample/basics.test.yaml»   4/4   0.6s   «dim|listsUsers · createsUser · getsCreatedUser · rejectsMissingEmail»",
        "  «b|Tests»  «ok b|4 passed» «dim|(4)»   «b|Time» 0.6s «dim|· demo service started and stopped by the test file»",
        "",
    ]
    ops = [
        ("create", "POST", "/users", "createUser", "cleanup → users.delete (DELETE on the same resource)"),
        ("get", "GET", "/users/{id}", "getUser", ""),
        ("list", "GET", "/users", "listUsers", "query: limit, status"),
        ("update", "PUT", "/users/{id}", "updateUser", ""),
        ("delete", "DELETE", "/users/{id}", "deleteUser", ""),
        ("startExport", "POST", "/users/exports", "startUserExport", "saves jobId"),
        ("exportStatus", "GET", "/users/exports/{jobId}", "getUserExportStatus", ""),
    ]
    L += [
        P("vc import openapi https://users.staging.shop.example/v3/api-docs --id users --tests"),
        "  «ok|✓» fetched  https://users.staging.shop.example/v3/api-docs  «dim|JSON · OpenAPI 3.0.3 · 7 operations · 5 schemas · 184ms»",
        "  «ok|wrote»  " + pad("services/users/users.api.yaml", 34) + "«b|7 apis»  «dim|· id} users «dim|· ids from operationId»".replace("}", "»"),
        "         " + row(["«dim|id»", "«dim|method»", "«dim|path»", "«dim|operationId»"], [20, 9, 27, 22]),
    ]
    for oid, m, p, op, note in ops:
        L.append("         " + row(["users.«b|" + oid + "»", m, p, "«dim|" + op + "»", "«dim|" + note + "»" if note else ""], [20, 9, 27, 22]))
    L += [
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
        L.append("      " + row(["«ok|✓» " + n, "«dim|" + m + "»", "«dim|" + p + "»", "«ok|" + st + "»", pad(" " * (6 - len(t)) + t, 9), "«dim|" + note + "»" if note else ""], [18, 8, 26, 5, 9]))
    L += [
        "",
        "  «b|Tests»  «ok b|7 passed» «dim|(7)»   «b|Time» 1.92s «dim|· 14 requests · cleanup users.delete ×4»",
        "  «dim|reports» .vibranium/reports/ «dim|→» ctrf.json · junit.xml · report.html",
        HINT + "vc report --open «dim|to browse requests ·} vc watch users «dim|reruns on save}".replace("}", "»"),
    ]
    return write("CliStart.dc.html", "Vibranium CLI · start from an OpenAPI spec", L)


if WHICH == "start":
    start()


TUI_TPL = """<!doctype html>
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
.pane{border:1px solid #2C313C;border-radius:4px;position:relative;padding:14px 12px 10px;box-sizing:border-box;min-width:0;overflow:visible}
.pane.focus{border-color:#A99BFF}
.ptitle{position:absolute;top:-9px;left:10px;background:#0F1115;padding:0 6px;color:#A99BFF;font-weight:700}
.ln{white-space:pre;line-height:20px}
.dim{color:#8B92A0}.ok{color:#4FC27A}.bad{color:#F06A5D}.warn{color:#E5A13A}.info{color:#6AA2F5}.vio{color:#A99BFF}.b{font-weight:700}
.k{color:#8FB8F0}.s{color:#C9D99A}.t{color:#E8B86D}
.sel{background:#252A36}
.edit{background:#1C2029;outline:1px solid #A99BFF}
.g{display:inline-block;width:1ch;text-align:center}
.cur{background:#D7DAE0;color:#0F1115}
.key{display:inline-block;background:#2C313C;color:#D7DAE0;padding:0 5px;border-radius:3px;margin-right:4px}
</style>
</helmet>
<div style="width: 1280px; height: 800px; box-sizing: border-box; background: #0F1115; color: #D7DAE0; font-family: 'JetBrains Mono', monospace; font-size: 13px; padding: 14px 16px; display: flex; flex-direction: column; gap: 12px">

  <div class="ln" style="display: flex; justify-content: space-between"><span>%(head)s</span><span><span class="dim">1 Run</span>  <span class="sel" style="padding: 0 8px">2 Explore</span>  <span class="dim">3 History</span>  <span class="dim">4 Load</span></span></div>

  <div class="ln" style="border: 1px solid #A99BFF; border-radius: 4px; padding: 4px 10px">%(bar)s</div>

  <div style="flex: 1; display: grid; grid-template-columns: 376px minmax(0, 1fr) 440px; gap: 14px; min-height: 0">

    <div class="pane focus">
      <span class="ptitle">%(t1)s</span>
%(left)s
    </div>

    <div class="pane">
      <span class="ptitle">%(t2)s</span>
%(mid)s
    </div>

    <div style="display: grid; grid-template-rows: minmax(0, 1fr) 286px; gap: 14px; min-height: 0">
      <div class="pane">
        <span class="ptitle">%(t3)s</span>
%(api)s
      </div>
      <div class="pane">
        <span class="ptitle">%(t4)s</span>
%(flow)s
      </div>
    </div>
  </div>

  <div class="ln dim">%(keys)s</div>
</div>
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{"$preview":{"width":1280,"height":800}}'>
class Component extends DCLogic {
  renderVals() {
    return {};
  }
}
</script>
</body>
</html>
"""


def tl(lines, indent, maxw, name, maxn):
    out = []
    for l in lines:
        sel = l.startswith("@")
        if sel:
            l = l[1:]
        if vis(l) > maxw:
            raise SystemExit("%s: line too long (%d > %d): %s" % (name, vis(l), maxw, l))
        out.append(" " * indent + '<div class="ln%s">%s</div>' % (" sel" if sel else "", render(l) if l.strip() else " "))
    if len(lines) > maxn:
        raise SystemExit("%s: %d lines > %d" % (name, len(lines), maxn))
    print(name, len(lines), "lines, maxw", max(vis(l.lstrip("@")) for l in lines))
    return "\n".join(out)


def tui():
    X = "«ok|[x]»"
    O = "«dim|[ ]»"
    EW = [4, 3, 5, 24]

    def E(chk, n, m, p, st, note=""):
        return "  " + row([chk, n, m, p, "«ok|" + st + "»" if st[0] == "2" else st, note], EW + [4])

    left = [
        "«dim|hide}  «vio|▣} static «dim|21}  «vio|▣} 3rd-party «dim|6}  «vio|▣} OPTIONS «dim|5}".replace("}", "»"),
        "«dim|14 of 46 shown · 13 selected · by host»",
        "",
        "«dim|▾» «b|users.staging.shop.example»        «dim|2»",
        E(X, "07", "GET", "/users/u_7f3k29qp", "200"),
        E(X, "08", "GET", "/users/u_7f…/addresses", "200"),
        "«dim|▾» «b|carts.staging.shop.example»        «dim|6»",
        E(X, "12", "POST", "/carts", "201"),
        "@" + E(X, "15", "POST", "/carts/c_4Hq9Lm/items", "201", "«info|×3»"),
        E(X, "18", "POST", "«dim|/carts/c_4Hq9Lm/items»", "201", "«dim|↳15»"),
        E(X, "21", "POST", "«dim|/carts/c_4Hq9Lm/items»", "201", "«dim|↳15»"),
        E(X, "22", "GET", "/carts/c_4Hq9Lm", "200", "«info|×2»"),
        E(X, "30", "GET", "«dim|/carts/c_4Hq9Lm»", "200", "«dim|↳22»"),
        "«dim|▾» «b|payments.staging.shop.example»     «dim|2»",
        E(O, "«dim|27»", "«dim|GET»", "«dim|/payment-methods»", "«dim|200»"),
        E(X, "32", "POST", "/authorizations«warn|?api_key»", "201"),
        "«dim|▾» «b|orders.staging.shop.example»       «dim|4»",
        E(X, "35", "POST", "/orders", "201"),
        E(X, "38", "GET", "/orders/ord_9Tk2Px", "200", "«info|×3»"),
        E(X, "41", "GET", "«dim|/orders/ord_9Tk2Px»", "200", "«dim|↳38»"),
        E(X, "44", "GET", "«dim|/orders/ord_9Tk2Px»", "200", "«dim|↳38»"),
        "",
        "«dim|▸ 21 static assets  .js .css .woff2 .png»",
        "«dim|▸ 6 third-party     analytics, chat widget»",
        "«dim|▸ 5 OPTIONS         CORS preflights»",
        "",
        "«dim|──────────────────────────────────────────»",
        "«b|13 selected» «dim|→» 8 apis «dim|(»«ok|5 new» «dim|·» 3 match«dim|)»",
        "«dim|27 unchecked: no later request uses it»",
    ]

    MW = [9]
    mid = [
        "«info b|POST» carts.staging.shop.example",
        "     /api/v1/carts/c_4Hq9Lm/items  «ok|201» «dim|92ms»",
        "",
        row(["«dim|file»", "services/carts/carts.api.yaml"], MW),
        row(["", "«ok|new file» «dim|· id carts · from the host»"], MW),
        row(["«dim|baseUrl»", "«s|${var.cartsUrl}»/api/v1"], MW),
        "",
        row(["«dim|id»", "carts.«edit|addItem»«cur| »"], MW),
        row(["", "«ok|✓» camelCase  «ok|✓» verb first  «ok|✓» unique"], MW),
        row(["", "«dim|rule: <file id>.<verb><Thing>»"], MW),
        row(["", "«dim|like carts.create, users.startExport»"], MW),
        "",
        row(["«dim|params»", "«dim|path» c_4Hq9Lm   → «s|${var.cartId}»"], MW),
        row(["", "«dim|      saved from entry 12 body.id»"], MW),
        row(["", "«dim|body» sku «s|\"SKU-1042\"» → «s|${var.sku}»"], MW),
        row(["", "«dim|    » quantity «t|1» → «s|${var.quantity | int}»"], MW),
        "",
        row(["«dim|merge»", "«info|×3} with entries 18, 21".replace("}", "»")], MW),
        row(["", "«dim|same method and path; only sku differs»"], MW),
        row(["", "«dim|→ one api; the flow step uses» «k|forEach»"], MW),
        "",
        row(["«dim|matches»", "«dim|none: carts has no api file yet»"], MW),
        row(["", "«dim|matched by method + path template, e.g.»"], MW),
        row(["", "«dim|entry 35 →» orders.place «dim|(existing)»"], MW),
        "",
        row(["«dim|secrets»", "«warn|Authorization: Bearer} → auth «b|shopOauth}".replace("}", "»")], MW),
        row(["", "«warn|Cookie: session} → dropped".replace("}", "»")], MW),
        "",
        row(["«dim|expect»", "«k|status»: «t|201» «dim|(all 3 recorded)»"], MW),
    ]

    api = [
        "«dim| 1» «k|id»: «s|carts»",
        "«dim| 2» «k|name»: «s|Carts»",
        "«dim| 3» «k|baseUrl»: «s|${var.cartsUrl}/api/v1»",
        "«dim| 4» «k|auth»: «s|shopOauth»",
        "«dim| 5» «k|apis»:",
        "«dim| 6»   - { «k|id»: «s|create», «k|method»: «s|POST», «k|path»: «s|/carts» }",
        "@«dim| 7»   - «k|id»: «s|addItem»",
        "@«dim| 8»     «k|name»: «s|Add item»",
        "@«dim| 9»     «k|method»: «s|POST»",
        "@«dim|10»     «k|path»: «s|/carts/${var.cartId}/items»",
        "@«dim|11»     «k|vars»: { «k|sku»: «s|SKU-1042», «k|quantity»: «t|1» }",
        "@«dim|12»     «k|body»:",
        "@«dim|13»       «k|sku»: «s|${var.sku}»",
        "@«dim|14»       «k|quantity»: «s|${var.quantity | int}»",
        "@«dim|15»     «k|expect»: { «k|status»: «t|201» }",
        "«dim|16»   - { «k|id»: «s|get», «k|path»: «s|/carts/${var.cartId}» }",
        "«dim|new file · 3 apis · no secrets, no cookies»",
    ]

    flow = [
        "«dim|142» - «k|id»: «s|recordedCheckout»",
        "«dim|143»   «k|name»: «s|Recorded checkout (HAR)»",
        "«dim|144»   «k|standalone»: «t|true»",
        "«dim|145»   «k|vars»: { «k|skus»: [«s|SKU-1042», «s|SKU-2210», «s|SKU-0087»] }",
        "«dim|146»   «k|steps»:",
        "«dim|147»     - { «k|id»: «s|cart», «k|call»: «s|carts.create» }",
        "@«dim|148»     - { «k|id»: «s|items», «k|call»: «s|carts.addItem»,",
        "@«dim|149»         «k|forEach»: «s|${var.skus}» }",
        "«dim|150»     - { «k|id»: «s|review», «k|call»: «s|carts.get» }",
        "«dim|151»     - { «k|id»: «s|authorize», «k|call»: «s|payments.authorize» }",
        "«dim|152»     - { «k|id»: «s|place», «k|call»: «s|orders.place» }",
        "«dim|153»     - { «k|id»: «s|fetch», «k|call»: «s|orders.get», «k|poll»: «s|…» }",
        "«dim|saves: cartId, cartTotal, paymentToken, orderId»",
    ]

    keys = " ".join([
        '<span class="key">%s</span>%s' % (html.escape(k), html.escape(v)) for k, v in [
            ("↑↓", "move"), ("space", "toggle"), ("e", "edit id"), ("m", "merge"), ("/", "filter"), ("1-3", "hide groups"),
            ("tab", "switch pane"), ("enter", "import"), ("?", "help"), ("esc", "cancel"),
        ]
    ])
    head = render("«vio b|◆ vibranium»  shop  «dim|profile» staging  «dim|Explore ›» import har  «dim|~/Downloads/checkout-session.har · 46 entries»")
    bar = render("«vio|→» 13 requests «dim|→} 8 apis «dim|in} 4 files  «dim|·} flow «b|orders.checkout.recordedCheckout} «dim|· 6 steps ·} «warn|3 secrets removed}  «dim|· nothing written until} enter".replace("}", "»"))
    out = TUI_TPL % {
        "title": "Vibranium TUI · import a HAR",
        "head": head,
        "bar": bar,
        "t1": "HAR entries · 46",
        "t2": "Mapping · entry 15",
        "t3": html.escape("services/carts/carts.api.yaml · new"),
        "t4": html.escape("checkout.test.yaml · flow preview"),
        "left": tl(left, 6, 45, "left", 31),
        "mid": tl(mid, 6, 48, "mid", 31),
        "api": tl(api, 8, 53, "api", 17),
        "flow": tl(flow, 8, 53, "flow", 13),
        "keys": keys,
    }
    with open(os.path.join(OUT, "TuiImport.dc.html"), "w") as f:
        f.write(out)
    print("TuiImport.dc.html height 800")


if WHICH == "tui":
    tui()
