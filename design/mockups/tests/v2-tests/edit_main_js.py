import sys
P = '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/project/Main.dc.html'
s = open(P).read()
def rep(a, b):
    global s
    if a not in s:
        print('MISSING:', a[:140]); sys.exit(1)
    s = s.replace(a, b, 1)

GEN = r'''
// The report index lists every test; passed tests beyond the hand-written ones above are listed from the same index.
const EXTRA_FILES = [
  ['users.profile', 'Profile', 'profile', 'profiles'], ['users.roles', 'Role', 'role', 'roles'], ['users.search', 'Account', 'account', 'accounts'],
  ['users.sessions', 'Session', 'session', 'sessions'], ['users.export', 'Export', 'export', 'exports'], ['orders.returns', 'Return', 'return', 'returns'],
  ['orders.invoices', 'Invoice', 'invoice', 'invoices'], ['orders.shipping', 'Shipment', 'shipment', 'shipments'], ['orders.status', 'StatusChange', 'status change', 'status changes'],
  ['catalog.inventory', 'StockItem', 'stock item', 'stock items'], ['catalog.categories', 'Category', 'category', 'categories'], ['catalog.images', 'Image', 'image', 'images'],
  ['catalog.reviews', 'Review', 'review', 'reviews'], ['catalog.prices', 'PriceList', 'price list', 'price lists'], ['carts.items', 'Line', 'cart line', 'cart lines'],
  ['carts.merge', 'GuestCart', 'guest cart', 'guest carts'], ['carts.expiry', 'IdleCart', 'idle cart', 'idle carts'], ['carts.coupons', 'Coupon', 'coupon', 'coupons'],
  ['geo.cities', 'City', 'city', 'cities'], ['geo.search', 'Place', 'place', 'places'], ['geo.timezones', 'Timezone', 'timezone', 'timezones'],
  ['payments.refunds', 'Refund', 'refund', 'refunds'], ['payments.webhooks', 'Webhook', 'webhook', 'webhooks'], ['payments.methods', 'PaymentMethod', 'payment method', 'payment methods'],
  ['payments.currency', 'ExchangeRate', 'exchange rate', 'exchange rates'], ['payments.limits', 'Limit', 'limit', 'limits'], ['payments.disputes', 'Dispute', 'dispute', 'disputes']
];
const NOUNS = {
  'users.crud': ['User', 'user', 'users'], 'users.reports': ['Report', 'report', 'reports'], 'users.auth': ['Token', 'token', 'tokens'],
  'orders.checkout': ['Order', 'order', 'orders'], 'orders.history': ['PastOrder', 'past order', 'past orders'], 'catalog.browse': ['Product', 'product', 'products'],
  'catalog.search': ['Result', 'search result', 'search results'], 'geo.groups': ['Group', 'location group', 'location groups'], 'carts.pricing': ['Cart', 'cart', 'carts'],
  'payments.cards': ['Card', 'card', 'cards']
};
EXTRA_FILES.forEach(([fid, N, n, ns]) => { NOUNS[fid] = [N, n, ns]; FILE_PATHS[fid] = 'services/' + fid.split('.')[0] + '/' + fid.split('.')[1] + '.test.yaml'; });
// [method, path suffix, status, id pattern, name pattern, check]
const TEMPLATES = [
  ['POST', '', 201, 'creates{N}', 'Creates {a} {n}', 'body.schema'],
  ['GET', '/{id}', 200, 'gets{N}', 'Gets {a} {n} by id', 'id: ${var.{v}Id}'],
  ['GET', '', 200, 'lists{Ns}', 'Lists {ns} newest first', 'items.length: gte 1'],
  ['PATCH', '/{id}', 200, 'updates{N}', 'Updates {a} {n}', 'updatedAt: exists'],
  ['DELETE', '/{id}', 204, 'deletes{N}', 'Deletes {a} {n}', 'body: empty'],
  ['POST', '', 422, 'rejectsMissingFields', 'Rejects {a} {n} with missing required fields', 'errors.length: gte 1'],
  ['GET', '/{id}', 404, 'returnsNotFoundForUnknown{N}', 'Returns 404 for an unknown {n}', 'error.code: not_found'],
  ['GET', '?page=2', 200, 'paginates{Ns}', 'Pages through {ns} 20 at a time', 'items.length: lte 20'],
  ['GET', '?sort=-createdAt', 200, 'sorts{Ns}ByDate', 'Sorts {ns} by creation date', 'items.every.createdAt: lte previous'],
  ['GET', '?status=active', 200, 'filters{Ns}ByStatus', 'Filters {ns} by status', 'items.every.status: active'],
  ['GET', '', 401, 'requiresToken', 'Rejects calls without a token', 'headers.www-authenticate: exists'],
  ['GET', '/{id}', 404, 'hides{N}FromOtherAccounts', 'Hides {a} {n} from other accounts', 'error.code: not_found'],
  ['GET', '/{id}', 200, 'returnsEtag', 'Returns an ETag with each {n}', 'headers.etag: exists'],
  ['PATCH', '/{id}', 412, 'rejectsStaleEtag', 'Rejects an update with a stale ETag', 'error.code: precondition_failed'],
  ['GET', '?limit=500', 200, 'capsPageSize', 'Caps the page size at 100', 'items.length: lte 100'],
  ['POST', '', 409, 'rejectsDuplicate{N}', 'Rejects a duplicate {n}', 'error.code: conflict'],
  ['GET', '?q=…', 200, 'finds{Ns}ByName', 'Finds {ns} by name', 'items[0].name: contains ${var.query}'],
  ['POST', '', 201, 'trimsWhitespace', 'Trims whitespace from text fields', 'name: ${var.name | trim}'],
  ['GET', '/{id}', 200, 'matches{N}Schema', '{A} {n} matches its JSON Schema', 'body.schema'],
  ['POST', '', 413, 'rejectsOversizedBody', 'Rejects a body over 1 MB', 'error.code: payload_too_large'],
  ['GET', '', 200, 'listsWithin300Ms', 'Answers the list call within 300 ms', 'timing.total: lt 300'],
  ['POST', '', 201, 'acceptsUnicodeNames', 'Accepts names with accents and non-Latin scripts', 'name: ${var.name}'],
  ['PUT', '/{id}', 200, 'replaces{N}', 'Replaces {a} {n} with PUT', 'body.match'],
  ['DELETE', '/{id}', 404, 'deletesTwiceSafely', 'Deleting {a} {n} twice returns 404', 'error.code: not_found'],
  ['GET', '?fields=id,name', 200, 'returnsSparseFields', 'Returns only the requested fields', 'items[0].createdAt: not exists'],
  ['POST', '', 400, 'rejectsMalformedJson', 'Rejects malformed JSON', 'error.code: bad_json'],
  ['GET', '', 200, 'setsCacheHeaders', 'Sets cache headers on the list', 'headers.cache-control: exists'],
  ['GET', '?page=999', 200, 'returnsEmptyLastPage', 'Returns an empty page past the end', 'items: empty'],
  ['PATCH', '/{id}', 422, 'rejectsUnknownStatus', 'Rejects an unknown status value', 'errors[0].field: status'],
  ['GET', '/{id}', 200, 'includesLinks', 'Includes self and collection links', 'links.self: exists'],
  ['POST', '', 201, 'recordsCreator', 'Records who created the {n}', 'createdBy: ${var.userId}'],
  ['GET', '', 200, 'reportsTotalCount', 'Reports the total count', 'total: gte 1'],
  ['POST', '', 201, 'returnsLocationHeader', 'Returns a Location header for a new {n}', 'headers.location: startsWith /'],
  ['GET', '?since=…', 200, 'lists{Ns}ChangedSince', 'Lists {ns} changed since a date', 'items.every.updatedAt: gte ${var.since}'],
  ['DELETE', '/{id}', 403, 'memberCannotDelete', 'A member cannot delete {a} {n}', 'error.code: forbidden'],
  ['GET', '/{id}', 304, 'honoursIfNoneMatch', 'Returns 304 for an unchanged {n}', 'body: empty'],
  ['POST', '', 201, 'createsWithRequiredFieldsOnly', 'Creates {a} {n} with only required fields', 'body.schema'],
  ['GET', '?page=1', 200, 'keepsStableOrder', 'Keeps a stable order across pages', 'items.length: 20'],
  ['POST', '', 429, 'rateLimitsBursts', 'Rate-limits more than 50 calls a second', 'headers.retry-after: exists'],
  ['GET', '/{id}', 200, 'returnsIsoDates', 'Returns dates in ISO 8601', 'createdAt: format date-time'],
  ['PATCH', '/{id}', 200, 'ignoresReadOnlyFields', 'Ignores read-only fields on update', 'id: ${var.{v}Id}'],
  ['GET', '', 200, 'usesProfileCurrency', 'Shows prices in the profile currency', 'currency: ${var.currency}'],
  ['POST', '', 201, 'honoursIdempotencyKey', 'A repeated request with an Idempotency-Key creates one {n}', 'id: ${var.firstId}'],
  ['GET', '?q=', 200, 'emptyQueryListsAll', 'An empty query lists everything', 'items.length: gte 1'],
  ['GET', '?status=unknown', 422, 'rejectsUnknownFilter', 'Rejects an unknown filter value', 'errors[0].field: status'],
  ['DELETE', '/{id}', 204, 'removesChildren', 'Deleting {a} {n} removes what belongs to it', 'body: empty'],
  ['GET', '/{id}/history', 200, 'keeps{N}History', 'Keeps a history of {n} changes', 'items.length: gte 1'],
  ['POST', '', 201, 'storesEmailInLowerCase', 'Stores emails in lower case', 'email: ${var.email | lower}']
];
const hashStr = (str) => { let h = 7; for (let i = 0; i < str.length; i++) h = (h * 31 + str.charCodeAt(i)) >>> 0; return h; };
function listedPassed() {
  const out = [];
  const have = new Set(TESTS.map((t) => t[1]));
  const need = {};
  FILES.forEach(([fid, , pass]) => { if (NOUNS[fid]) need[fid] = pass - TESTS.filter((t) => t[0] === 'passed' && t[1].indexOf(fid + '.') === 0).length; });
  EXTRA_FILES.forEach(([fid], i) => { need[fid] = i < 17 ? 8 : 7; });
  Object.keys(need).forEach((fid) => {
    const [N, n, ns] = NOUNS[fid];
    const v = N[0].toLowerCase() + N.slice(1);
    const Ns = N + (/y$/.test(N) ? '' : 's'); const NsFix = /y$/.test(N) ? N.slice(0, -1) + 'ies' : Ns;
    const a = /^[aeiou]/.test(n) ? 'an' : 'a';
    const base = '/' + fid.split('.')[0] + (fid.split('.')[1] === 'crud' ? '' : '/' + fid.split('.')[1]);
    let k = 0, made = 0;
    while (made < need[fid] && k < TEMPLATES.length) {
      const [m, suf, code, idp, namep, check] = TEMPLATES[k++];
      const id = fid + '.' + idp.replace('{Ns}', NsFix).replace('{N}', N);
      if (have.has(id)) continue;
      have.add(id); made++;
      const ms = 60 + (hashStr(id) % 700);
      const name = namep.replace('{A}', a === 'an' ? 'An' : 'A').replace('{a}', a).replace('{ns}', ns).replace('{n}', n);
      out.push(['passed', id, name, ms, '0', '1', '', fid.split('.')[0] + (k <= 3 ? ', smoke' : ''), 'none', '',
        [['ok', 'status: ' + code, ''], ['ok', check.replace('{v}', v), '']], [[m, base + suf.replace('{id}', '/{' + v + 'Id}').replace('/{id}', ''), code + ' · ' + ms + ' ms']], false]);
    }
  });
  return out;
}
const ALL_TESTS = TESTS.concat(listedPassed());
const PAGE = 50;
'''
rep("const mcls = (m) => 'm-' + m.toLowerCase();", GEN + "const mcls = (m) => 'm-' + m.toLowerCase();")

rep("this.state = { status: 'all', q: '', groupByFile: true, changedOnly: false, open: { 'orders.checkout.placesOrder': true }, sec: 'summary', copied: '', };",
    "this.state = { status: 'all', q: '', groupByFile: true, changedOnly: false, open: { 'orders.checkout.placesOrder': true }, loaded: { 'orders.checkout.placesOrder': true }, loading: {}, limit: PAGE, sec: 'summary', copied: '', sideCollapsed: false, moreOpen: false };")
rep('''    const rows = TESTS.map((t) => {''', '''    const loadRows = (ids) => {
      const st = this.state; const need = ids.filter((id) => !(st.loaded || {})[id]);
      if (!need.length) return {};
      setTimeout(() => {
        const lo = Object.assign({}, this.state.loaded), li = Object.assign({}, this.state.loading);
        need.forEach((id) => { lo[id] = true; li[id] = false; });
        this.setState({ loaded: lo, loading: li });
      }, 450);
      const li = Object.assign({}, st.loading); need.forEach((id) => { li[id] = true; });
      return { loading: li };
    };
    const rows = ALL_TESTS.map((t) => {''')
rep('''        open, openCls: open ? 'open' : '', toggleLabel: (open ? 'Collapse ' : 'Expand ') + id,
        toggle: () => this.setState({ open: Object.assign({}, this.state.open, { [id]: !this.state.open[id] }) }),''', '''        open, openCls: open ? 'open' : '', toggleLabel: (open ? 'Collapse ' : 'Expand ') + id,
        loading: !!(s.loading || {})[id], ready: !(s.loading || {})[id],
        page: 'tests/' + id + '.json · ' + (2 + checks.length + ex.length * 2 + (st === 'failed' ? 9 : 0)) + ' KB',
        toggle: () => {
          const opening = !this.state.open[id];
          this.setState(Object.assign({ open: Object.assign({}, this.state.open, { [id]: opening }) }, opening ? loadRows([id]) : {}));
        },''')
rep('''    let groups;
    if (s.groupByFile) {
      const byFile = {};
      shown.forEach((r) => {''', '''    const limit = s.limit || PAGE;
    const paged = shown.slice(0, limit);
    let groups;
    if (s.groupByFile) {
      const byFile = {};
      paged.forEach((r) => {''')
rep('''      groups = shown.length ? [{ id: '', path: '', showHead: false, rows: shown, summary: '' }] : [];''',
    '''      groups = paged.length ? [{ id: '', path: '', showHead: false, rows: paged, summary: '' }] : [];''')
rep('''    const anyOpen = shown.some((r) => r.open);''', '''    const anyOpen = paged.some((r) => r.open);''')
rep('''    const pick = (k) => () => this.setState({ status: k });''', '''    const pick = (k) => () => this.setState({ status: k, limit: PAGE });''')
rep('''      toggleChanged: (e) => this.setState({ changedOnly: e && e.target ? !!e.target.checked : !this.state.changedOnly }),''',
    '''      toggleChanged: (e) => this.setState({ changedOnly: e && e.target ? !!e.target.checked : !this.state.changedOnly, limit: PAGE }),''')
rep('''      q: s.q, onQ: (e) => this.setState({ q: e.target.value }),''', '''      q: s.q, onQ: (e) => this.setState({ q: e.target.value, limit: PAGE }),''')
rep('''      clearFilters: () => this.setState({ status: 'all', q: '', changedOnly: false }),''', '''      clearFilters: () => this.setState({ status: 'all', q: '', changedOnly: false, limit: PAGE }),''')
rep('''        else { const o = {}; shown.forEach((r) => { o[r.id] = true; }); this.setState({ open: o }); }''',
    '''        else { const o = {}; paged.forEach((r) => { o[r.id] = true; }); this.setState(Object.assign({ open: o }, loadRows(paged.map((r) => r.id)))); }''')
rep('''      shownText: `${shown.length} shown · ${groups.length} ${s.groupByFile ? 'files' : 'group'} · sorted by status, then id`,''',
    '''      shownText: `${shown.length} tests · sorted by status, then id`,
      pageText: shown.length > paged.length ? `Showing ${paged.length} of ${shown.length} · rows load in pages of ${PAGE}` : `Showing all ${shown.length}`,
      hasMore: shown.length > paged.length, nextCount: Math.min(PAGE, shown.length - paged.length), shownCount: shown.length,
      showMore: () => this.setState({ limit: limit + PAGE }), showAll: () => this.setState({ limit: shown.length }),
      appCls: 'app' + (s.sideCollapsed ? ' side-collapsed' : ''), sideOpen: !s.sideCollapsed, sideShut: !!s.sideCollapsed,
      collapseLabel: s.sideCollapsed ? 'Expand sidebar' : 'Collapse sidebar', toggleSide: () => this.setState({ sideCollapsed: !this.state.sideCollapsed }),
      moreOpen: !!s.moreOpen, toggleMore: () => this.setState({ moreOpen: !this.state.moreOpen }),''')
rep('''    const hiddenPassed = s.status === 'all' || s.status === 'passed';\n''', '')
rep('''      moreText: hiddenPassed ? '382 further passed tests from 27 green files are summarised under By file.' : 'Every test with this status is listed above.',\n''', '')
open(P, 'w').write(s)
print('js ok')
