/* Client · GraphQL (#client-graphql): query, variables, schema explorer, data and errors. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions;
  const { esc, ic, badge, method, chipSrc, urlTokens, layers, jsonTree, hlJson, hlYaml } = VC.h;
  const G = D.gql;
  Object.assign(S, { gqTab: 'query', gsTab: 'errors', schemaType: 'Product', gqlErrorsAdded: false });

  /* minimal GraphQL highlighter: keywords, $vars, types, directives, strings */
  function hlGql(src) {
    return VC.h.tokenize(src, /(#.*$)|\b(query|mutation|subscription|fragment|on|true|false|null)\b|(\$\w+)|(@\w+)|("[^"]*")|\b([A-Z]\w*!?)|(\b\d+\b)/gm, (m) => {
      if (m[1]) return `<span class="c">${esc(m[1])}</span>`;
      if (m[2]) return `<span class="kw">${m[2]}</span>`;
      if (m[3]) return `<span class="k">${esc(m[3])}</span>`;
      if (m[4]) return `<span class="b">${esc(m[4])}</span>`;
      if (m[5]) return `<span class="s">${esc(m[5])}</span>`;
      if (m[6]) return `<span class="ty">${esc(m[6])}</span>`;
      return `<span class="n">${m[7]}</span>`;
    });
  }
  const numbered = (html) => html.split('\n').map((l, i) => `<span class="ln">${i + 1}</span>${l}`).join('\n');

  function schemaPanel() {
    const t = G.schema.find((x) => x.name === S.schemaType);
    return `<div class="schema glass-dense">
      <div class="schema-types" role="listbox" aria-label="Types">${G.schema.map((x) => `<button class="schema-type mono" role="option" aria-selected="${x.name === S.schemaType}" data-action="gql-type" data-v="${x.name}" data-fk="st-${x.name}">${esc(x.name)}<span class="kind">${x.kind}</span></button>`).join('')}</div>
      <div class="schema-fields" data-sk="schema-fields"><div class="row"><b class="mono">${t.kind === 'type' ? 'type' : t.kind} ${esc(t.name)}</b><span class="grow"></span><span class="subtle">introspected 14:02 · 38 types</span></div>
        ${t.fields.map((f) => `<div class="schema-field"><span class="fname">${esc(f.n)}</span><span class="args">${esc(f.a)}</span>${f.t ? `: <span class="ftype">${esc(f.t)}</span>` : ''}${f.dir ? ` <span class="dir">${esc(f.dir)}</span>` : ''}${f.d ? `<p>${esc(f.d)}</p>` : ''}</div>`).join('')}
      </div></div>`;
  }
  function reqPanel() {
    switch (S.gqTab) {
      case 'query': return `<div class="gql-editor"><pre class="code glass-dense" aria-label="Query" tabindex="0">${numbered(hlGql(G.query))}</pre></div>
        <div class="row"><span class="subtle">operationName: ProductById · from catalog.api.yaml:10 · ⌃Space completes from the schema</span></div>`;
      case 'variables': return `<pre class="code glass-dense" aria-label="Variables" tabindex="0">${numbered(hlJson(G.variables))}</pre>
        ${layers({ inherited: [{ k: 'id', v: '${productId}', src: 'api', at: 'catalog.api.yaml:14' }], own: [{ k: 'withCost', v: 'true' }] }, 'gvars')}`;
      case 'schema': return schemaPanel();
      case 'headers': return layers({ inherited: [{ k: 'authorization', v: 'Bearer ••••', src: 'profile', at: 'auth: shopAuth' }, { k: 'x-client', v: 'vibranium', src: 'config', at: 'vibranium.config.yaml:11' }], own: [] }, 'gheaders');
      default: return `<pre class="code glass-dense">${hlYaml(expectYaml())}</pre>`;
    }
  }
  const expectYaml = () => 'expect:\n  status: 200\n  body:\n    data.product.id: ${productId}' + (S.gqlErrorsAdded ? '\n  errors:\n    - code: FORBIDDEN\n      path: product.costPrice' : '');

  function errorsPanel() {
    const e = G.data.errors[0];
    return `${S.gqlErrorsAdded ? `<p class="callout" data-tone="ok">${ic('pass')}<span><b>Expected.</b> The FORBIDDEN error on product.costPrice is checked by <code>expect.errors</code>; V5128 no longer applies.</span></p>`
      : `<p class="callout" data-tone="warn">${ic('warn')}<span><b>V5128 · partial data, and no errors check.</b> The response has <code>data</code> and one error. help: add <code>expect.errors</code> so a new error fails the test, or <code>errors: none</code> to forbid them. fix: the button below adds the error it saw.</span></p>`}
      <div class="table-wrap glass-dense"><table class="table"><thead><tr><th scope="col">Code</th><th scope="col">Path</th><th scope="col">Message</th></tr></thead>
      <tbody><tr><td><span class="chip">${esc(e.extensions.code)}</span></td><td class="err-path">${esc(e.path.join('.'))}</td><td>${esc(e.message)}<div class="subtle">requires ${esc(e.extensions.requires)}</div></td></tr></tbody></table></div>
      <section class="layer"><div class="layer-head"><h3 class="section-label">errors[0] · raw</h3></div>${jsonTree(e, { root: 'errors[0]', sk: 'gql-err' })}</section>
      ${S.gqlErrorsAdded ? '' : `<div class="row"><button class="btn" data-action="gql-add-errors" data-fk="gql-add-errors">${ic('plus')}Add expect.errors</button><span class="subtle">Shows the diff first</span></div>`}`;
  }
  function respPanel() {
    switch (S.gsTab) {
      case 'data': return jsonTree(G.data.data, { root: 'data', sk: 'gql-data' });
      case 'errors': return errorsPanel();
      case 'headers': return `<div class="table-wrap glass-dense"><table class="table"><tbody>${[['content-type', 'application/graphql-response+json'], ['x-request-id', '9f1d2c3b-7a1e'], ['server', 'vc-demo/2.0']].map(([k, v]) => `<tr><td class="mono">${k}</td><td class="mono">${v}</td></tr>`).join('')}</tbody></table></div>`;
      case 'checks': return `<div class="table-wrap glass-dense"><table class="table"><thead><tr><th scope="col">Result</th><th scope="col">Check</th><th scope="col">Got</th><th scope="col">From</th></tr></thead><tbody>
        <tr><td>${badge('pass', null, true)}</td><td class="mono">status: 200</td><td class="mono">200</td><td>${chipSrc('api')}</td></tr>
        <tr><td>${badge('pass', null, true)}</td><td class="mono">data.product.id</td><td class="mono">"p_1042"</td><td>${chipSrc('api')}</td></tr>
        <tr><td>${S.gqlErrorsAdded ? badge('pass', null, true) : badge('allow', 'warn', true)}</td><td class="mono">errors</td><td class="mono">1 error</td><td>${S.gqlErrorsAdded ? chipSrc('request') : '<span class="chip">V5128</span>'}</td></tr></tbody></table></div>`;
      default: return `<pre class="code glass-dense">${hlJson(G.data)}</pre>`;
    }
  }
  const ensure = () => { if (VC.api().m !== 'GQL') S.api = 'catalog#product'; return VC.api(); };
  function render() {
    const a = ensure();
    const errs = S.gqlErrorsAdded ? 'is-good' : 'is-bad';
    return `<div class="client">${VC.reqbar({ m: 'GQL', typeLabel: 'GraphQL', url: '${baseUrl}' + a.path })}
      <div class="split"${VC.splitAttrs()}>
        <section class="glass" aria-label="Operation">
          <div class="card-head"><div class="req-name">${method('GQL')}<h2 class="card-title">${esc(a.name)}</h2><span class="file-ref hide-narrow">${esc(S.api)} · catalog.api.yaml:${a.line}</span></div>
            <button class="btn btn-sm" data-action="toast" data-msg="Saved as test" data-sub="catalog.read#readsProduct updated">${ic('runs')}Save as test</button></div>
          <div class="card-tabs">${VC.tabs('gq', 'Operation', [['query', 'Query'], ['variables', 'Variables', '<span class="count">2</span>'], ['schema', 'Schema'], ['headers', 'Headers', '<span class="count">2</span>'], ['expect', 'Expect']], S.gqTab, 'gq-tab')}</div>
          ${VC.panel('gq', S.gqTab, reqPanel())}
        </section>
        ${VC.divider('Resize operation and response')}
        <section class="glass" aria-label="Response">
          <div class="card-head"><div class="resp-meta grow"><span class="hs" data-class="2">200</span><span class="stat">OK</span><span class="stat"><b>96</b> ms</span><span class="stat"><b>612 B</b></span>
            ${S.gqlErrorsAdded ? badge('pass', 'errors expected', true) : badge('allow', 'partial data', true)}</div>${VC.orientToggle()}</div>
          <div class="card-tabs">${VC.tabs('gs', 'Response', [['data', 'Data'], ['errors', 'Errors', `<span class="count ${errs}">1</span>`], ['headers', 'Headers'], ['checks', 'Checks'], ['raw', 'Raw']], S.gsTab, 'gs-tab')}</div>
          ${VC.panel('gs', S.gsTab, respPanel())}
        </section>
      </div></div>`;
  }
  A['gq-tab'] = (el) => { S.gqTab = el.dataset.v; VC.render(); };
  A['gs-tab'] = (el) => { S.gsTab = el.dataset.v; VC.render(); };
  A['gql-type'] = (el) => { S.schemaType = el.dataset.v; VC.render(); };
  A['gql-add-errors'] = () => VC.diffDialog({
    title: 'Add expect.errors', sub: 'Checks the error this response returned, so V5128 stops warning and a new error fails the test.',
    file: 'services/catalog/read.test.yaml',
    lines: [['@', '@@ -18,6 +18,9 @@ tests:'], [' ', '  - id: readsProduct'], [' ', '    call: catalog#product'], [' ', '    expect:'], [' ', '      body:'], [' ', '        data.product.id: ${productId}'],
      ['+', '      errors:'], ['+', '        - code: FORBIDDEN'], ['+', '          path: product.costPrice']],
    confirm: 'Write test',
    after: () => { S.gqlErrorsAdded = true; VC.render(); }
  });

  VC.screens['client-graphql'] = {
    enter: ensure,
    crumbs: () => [D.project.name, 'catalog', VC.api().name],
    status: () => `${ic('clock', 'ico-sm')} last sent <b>13:41</b> · <span class="hs" data-class="2">200</span> · 1 error`,
    render,
    mount: (v) => VC.mountSplit(v),
    send: () => VC.toast('200 OK · 96 ms', S.gqlErrorsAdded ? 'data and 1 expected error' : 'Partial data: 1 error, V5128')
  };
})();
