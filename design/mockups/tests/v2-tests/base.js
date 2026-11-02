const path = require('path');
const { loadComponent } = require('./load');
const { makeRenderer } = require('./render');
for (const n of ['Main', 'ReportFailure', 'ReportLoad']) {
  const ART = path.resolve(__dirname, '../vibmock/project/' + n + '.dc.html');
  const { Component } = loadComponent(ART);
  const r = makeRenderer(ART, path.join(__dirname, 'shots-base'));
  const c = new Component({});
  r.save(n, c.renderVals());
  console.log(n, r.errors().slice(0, 5));
}
