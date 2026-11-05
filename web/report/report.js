// The Vibranium HTML report (D169). No dependencies; works from file://.
//
// Data arrives as `VIB.load(key, value)` calls: in the folder layout each key is the script
// `data/<key>.js`, loaded on demand; in the single-file layout every key is in one gzip+base64
// blob (`#vib-data`) inflated with DecompressionStream. Keys: `summary`, `index`, `tests/<shard>`
// and `bodies/<hash>`. Everything from the data is rendered with textContent, never as HTML.
"use strict";
(function () {
  var ROW = 32;
  var STATUSES = ["passed", "failed", "blocked", "flaky", "skipped"];
  var store = {};
  var waiting = {};
  var bundle = null;
  var state = { rows: [], shown: [], selected: -1, filter: "", statuses: {} };

  var VIB = (window.VIB = {
    load: function (key, value) {
      store[key] = value;
      (waiting[key] || []).forEach(function (done) { done(value); });
      delete waiting[key];
    },
    get: function (key) {
      if (key in store) return Promise.resolve(store[key]);
      if (bundle) return bundle.then(function (all) { return all[key]; });
      return new Promise(function (resolve, reject) {
        var first = !waiting[key];
        (waiting[key] = waiting[key] || []).push(resolve);
        if (!first) return;
        var script = document.createElement("script");
        script.src = "data/" + key + ".js";
        script.onerror = function () { reject(new Error("missing " + script.src)); };
        document.body.appendChild(script);
      });
    },
  });

  function el(tag, cls, text) {
    var node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text !== undefined && text !== null) node.textContent = String(text);
    return node;
  }

  function inflate() {
    var source = document.getElementById("vib-data");
    if (!source) return null;
    var bytes = Uint8Array.from(atob(source.textContent.trim()), function (c) { return c.charCodeAt(0); });
    var stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"));
    return new Response(stream).json();
  }

  function ms(value) {
    return value === null || value === undefined ? "" : value < 10 ? value.toFixed(1) : Math.round(value).toString();
  }

  function header(summary) {
    var run = summary.run;
    document.getElementById("run-title").textContent = "run " + run.id;
    var badge = document.getElementById("run-status");
    badge.textContent = run.status;
    badge.className = "badge s-" + run.status;
    var meta = [new Date(run.startedMs).toLocaleString(), ms(run.durationMs) + " ms"];
    if (run.profiles.length) meta.push("profiles: " + run.profiles.join(", "));
    if (run.seed !== null && run.seed !== undefined) meta.push("seed " + run.seed);
    document.getElementById("run-meta").textContent = meta.join(" · ");
    var totals = document.getElementById("totals");
    STATUSES.forEach(function (status) {
      var box = el("div", "total");
      box.appendChild(el("b", "s-" + status, summary.totals[status] || 0));
      box.appendChild(el("span", "", status));
      totals.appendChild(box);
    });
    lifecycle(summary.lifecycle || []);
  }

  function lifecycle(items) {
    if (!items.length) return;
    var section = document.getElementById("lifecycle");
    section.hidden = false;
    section.appendChild(el("strong", "", "Lifecycle and cleanups"));
    var list = el("ul");
    items.forEach(function (item) {
      var line = el("li");
      line.appendChild(el("span", "s-" + item.status, item.status + " "));
      line.appendChild(el("span", "", item.label + (item.message ? ": " + item.message : "")));
      list.appendChild(line);
    });
    section.appendChild(list);
  }

  function chips() {
    var box = document.getElementById("chips");
    STATUSES.forEach(function (status) {
      var chip = el("button", "chip s-" + status, status);
      chip.type = "button";
      chip.setAttribute("aria-pressed", "false");
      chip.onclick = function () {
        state.statuses[status] = !state.statuses[status];
        chip.setAttribute("aria-pressed", String(!!state.statuses[status]));
        refilter();
      };
      box.appendChild(chip);
    });
    document.getElementById("search").oninput = function (event) {
      state.filter = event.target.value.toLowerCase();
      refilter();
    };
  }

  function matches(row) {
    var any = STATUSES.some(function (s) { return state.statuses[s]; });
    if (any && !state.statuses[row[3]]) return false;
    if (!state.filter) return true;
    var text = (row[0] + " " + row[1] + " " + row[5].join(" ")).toLowerCase();
    return text.indexOf(state.filter) !== -1;
  }

  function refilter() {
    state.shown = [];
    for (var i = 0; i < state.rows.length; i++) if (matches(state.rows[i])) state.shown.push(i);
    document.getElementById("spacer").style.height = state.shown.length * ROW + "px";
    document.getElementById("count").textContent = state.shown.length + " of " + state.rows.length + " tests";
    paint();
  }

  function paint() {
    var viewport = document.getElementById("viewport");
    var first = Math.max(0, Math.floor(viewport.scrollTop / ROW) - 10);
    var last = Math.min(state.shown.length, first + Math.ceil(viewport.clientHeight / ROW) + 20);
    var rows = document.getElementById("rows");
    rows.style.transform = "translateY(" + first * ROW + "px)";
    rows.replaceChildren();
    for (var i = first; i < last; i++) rows.appendChild(line(state.shown[i]));
  }

  function line(number) {
    var row = state.rows[number];
    var node = el("div", "row test" + (number === state.selected ? " selected" : ""));
    var name = el("span", "s-" + row[3]);
    name.appendChild(el("i", "dot"));
    name.appendChild(document.createTextNode(row[1] || row[0]));
    name.title = row[0];
    node.appendChild(name);
    node.appendChild(el("span", "", state.files[row[2]]));
    node.appendChild(el("span", "num", ms(row[4])));
    node.onclick = function () { location.hash = "#/test/" + encodeURIComponent(row[0]); };
    return node;
  }

  function route() {
    var match = /^#\/test\/(.+)$/.exec(location.hash);
    if (!match) return;
    var id = decodeURIComponent(match[1]);
    var number = state.rows.findIndex(function (row) { return row[0] === id; });
    if (number === -1) return;
    state.selected = number;
    paint();
    show(number);
  }

  function show(number) {
    var row = state.rows[number];
    var shard = Math.floor(number / state.shardSize);
    VIB.get("tests/" + shard).then(function (tests) {
      if (state.selected === number) detail(row, tests[String(number)] || {});
    });
  }

  function detail(row, test) {
    var pane = document.getElementById("detail");
    pane.replaceChildren();
    pane.appendChild(el("h2", "s-" + row[3], row[1] || row[0]));
    pane.appendChild(el("div", "id", row[0] + " · " + row[3] + " · " + ms(row[4]) + " ms"));
    if (test.location) pane.appendChild(el("div", "loc", test.location));
    if (row[5].length) pane.appendChild(el("div", "note", "tags: " + row[5].join(", ")));
    if (test.blockedBy) pane.appendChild(el("p", "note", "blocked by " + test.blockedBy));
    if (test.skipReason) pane.appendChild(el("p", "note", "skipped: " + test.skipReason));
    if (test.message) pane.appendChild(el("div", "message", test.message));
    checks(pane, test.checks || []);
    exchanges(pane, test.exchanges || []);
  }

  function checks(pane, list) {
    if (!list.length) return;
    pane.appendChild(el("h3", "", "Checks"));
    list.forEach(function (check) {
      var node = el("div", "check");
      node.appendChild(el("span", check.passed ? "s-passed" : "s-failed", check.passed ? "✓" : "✗"));
      var body = el("div");
      body.appendChild(el("code", "", check.label + (check.path ? " " + check.path : "")));
      if (!check.passed) {
        body.appendChild(el("div", "note", "expected " + check.expected));
        if (check.actual !== null && check.actual !== undefined) body.appendChild(el("div", "note", "actual " + check.actual));
      }
      if (check.location) body.appendChild(el("div", "loc", check.location));
      node.appendChild(body);
      pane.appendChild(node);
    });
  }

  function exchanges(pane, list) {
    if (!list.length) return;
    pane.appendChild(el("h3", "", "Exchanges"));
    list.forEach(function (exchange) {
      var box = el("details", "exchange");
      var status = exchange.status === null ? "no response" : exchange.status;
      box.appendChild(el("summary", "", exchange.method + " " + exchange.url + " → " + status + " · " + ms(exchange.ms) + " ms"));
      var body = el("div", "body");
      headers(body, "Request headers", exchange.requestHeaders);
      payload(body, "Request body", exchange.requestBody);
      headers(body, "Response headers", exchange.responseHeaders);
      payload(body, "Response body", exchange.responseBody);
      if (exchange.truncated) body.appendChild(el("p", "note", "Bodies were cut at history.maxBodySize."));
      box.appendChild(body);
      pane.appendChild(box);
    });
  }

  function headers(parent, title, list) {
    if (!list || !list.length) return;
    parent.appendChild(el("div", "note", title));
    var table = el("table");
    list.forEach(function (pair) {
      var tr = el("tr");
      tr.appendChild(el("td", "", pair[0]));
      tr.appendChild(el("td", "", pair[1]));
      table.appendChild(tr);
    });
    parent.appendChild(table);
  }

  function payload(parent, title, hash) {
    if (!hash) return;
    parent.appendChild(el("div", "note", title));
    var pre = el("pre", "", "…");
    parent.appendChild(pre);
    VIB.get("bodies/" + hash).then(function (text) { pre.textContent = pretty(text); }, function () { pre.textContent = "(missing)"; });
  }

  function pretty(text) {
    try { return JSON.stringify(JSON.parse(text), null, 2); } catch (_) { return text; }
  }

  VIB.start = function () {
    bundle = inflate();
    Promise.all([VIB.get("summary"), VIB.get("index")]).then(function (loaded) {
      var summary = loaded[0];
      state.rows = loaded[1];
      state.files = summary.files;
      state.shardSize = summary.shardSize;
      header(summary);
      chips();
      var viewport = document.getElementById("viewport");
      viewport.onscroll = function () { window.requestAnimationFrame(paint); };
      refilter();
      window.onhashchange = route;
      route();
    });
  };
})();
