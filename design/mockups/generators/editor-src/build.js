const fs = require('fs'), path = require('path');
const SRC = __dirname, OUT = path.join(SRC, '../vibmock/project/WebEditor.dc.html');
const seedRoot = path.join(SRC, 'seed'), seed = {};
(function walk(d) { for (const f of fs.readdirSync(d).sort()) { const p = path.join(d, f); if (fs.statSync(p).isDirectory()) walk(p); else seed[path.relative(seedRoot, p)] = fs.readFileSync(p, 'utf8'); } })(seedRoot);
const order = ['vibranium.config.yaml'];
const seedObj = {}; Object.keys(seed).sort((a, b) => (order.indexOf(b) - order.indexOf(a)) || (a < b ? -1 : 1)).forEach(k => seedObj[k] = seed[k]);
const parts = ['logic1.js', 'logic2.js', 'logic3.js', 'logic4.js', 'logic5.js', 'logic6.js', 'comp.js', 'view.js'].map(f => fs.readFileSync(path.join(SRC, f), 'utf8'));
const names = [];
parts.join('\n').replace(/^(?:function|var) ([A-Za-z_$][\w$]*)/gm, (_, n) => { if (names.indexOf(n) < 0) names.push(n); });
const script = ['// Vibranium web editor artboard. Pure logic first (tested in Node from this file), then the component.',
  '// Everything is scoped inside one function so nothing leaks onto window; the class is returned.',
  'var Component = (function () {', '\'use strict\';',
  'var SEED = ' + JSON.stringify(seedObj, null, 0) + ';'].concat(parts).concat([
  'Component.logic = { ' + names.map(n => n + ': ' + n).join(', ') + ' };',
  'return Component;', '})();']).join('\n');
if (/<\/script/i.test(script)) throw new Error('script contains </script');
const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Vibranium editor</title>
<script src="./support.js"></script>
<link rel="stylesheet" href="/_blob/81dcd3b549d078b575adb6aed82e1861">
<script src="/_blob/5809f53fc2a29e786d4c63f8d536045b"></script>
</head>
<body>
<x-dc>
<helmet>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&amp;family=Geist+Mono:wght@400;500;600&amp;display=swap">
<style>
${fs.readFileSync(path.join(SRC, 'style.css'), 'utf8').trim()}
</style>
</helmet>
${fs.readFileSync(path.join(SRC, 'template.html'), 'utf8').trim()}
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{"$preview":{"width":1440,"height":1000}}'>
${script}
</script>
</body>
</html>
`;
fs.writeFileSync(OUT, html);
console.log('wrote', OUT, html.length, 'bytes');
