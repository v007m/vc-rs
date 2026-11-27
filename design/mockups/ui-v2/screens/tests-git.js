/* Tests space · the Changes pane in the sidebar: the branch, ahead/behind, the
   changed test files (M/A/D with +/- counts), stage checkboxes, a commit message
   and Commit (Push, Pull and Commit & push sit in the ⋯).
   VC.gitPane() returns the section body, VC.gitCount() its badge. Data: TD.git. */
(function () {
  'use strict';
  const VC = window.VC, A = VC.actions, TD = VC.testsData;
  const { $, esc, ic } = VC.h;
  const G = TD.git;
  let msg = '';
  let sha = 0x41c9e2;

  const ST = { M: 'modified', A: 'added', D: 'deleted' };
  const counts = (lines) => lines.reduce(([a, d], [k]) => [a + (k === '+'), d + (k === '-')], [0, 0]);
  const staged = () => G.files.filter((f) => f.staged);
  const plural = (n, w) => `${n} ${w}${n === 1 ? '' : 's'}`;
  VC.gitCount = () => G.files.length;

  function branchHtml() {
    return `<div class="tg-branch" title="Tracking ${esc(G.upstream)}">${ic('branch', 'ico-sm')}<span class="mono ellipsis grow">${esc(G.branch)}</span>
      <span class="tg-ab num" aria-label="${plural(G.ahead, 'commit')} to push, ${plural(G.behind, 'commit')} to pull"><span${G.ahead ? ' class="is-on"' : ''}>${ic('up', 'ico-sm')}${G.ahead}</span><span${G.behind ? ' class="is-on"' : ''}>${ic('down', 'ico-sm')}${G.behind}</span></span></div>`;
  }
  function fileHtml(f, i) {
    const [a, d] = counts(f.lines);
    const cut = f.path.lastIndexOf('/');
    return `<li class="tg-file${f.staged ? '' : ' is-unstaged'}">
      <input type="checkbox" class="tg-stage" data-i="${i}" ${f.staged ? 'checked' : ''} aria-label="Stage ${esc(f.path)}" data-fk="tg-s-${i}">
      <button class="tg-open" data-action="git-diff" data-i="${i}" data-fk="tg-f-${i}" title="${esc(f.path)} · show the diff">
        <span class="tg-st tg-st-${f.st}" aria-label="${ST[f.st]}">${f.st}</span><span class="tg-name ellipsis">${esc(f.path.slice(cut + 1))}</span>
        <span class="tg-n num"><span class="add">+${a}</span><span class="del">−${d}</span></span></button></li>`;
  }
  function listHtml() {
    const n = staged().length, all = n === G.files.length;
    return `<div class="tg-head"><label class="tg-all"><input type="checkbox" class="tg-stage-all" ${all ? 'checked' : ''} aria-label="Stage all ${G.files.length} files" data-fk="tg-all"><span>${n} of ${G.files.length} staged</span></label></div>
      <ul class="tg-files" aria-label="Changed files">${G.files.map(fileHtml).join('')}</ul>`;
  }
  function commitHtml() {
    const n = staged().length;
    return `<div class="well tg-msg"><textarea class="tg-text" id="tg-msg" rows="2" placeholder="Commit message" aria-label="Commit message" data-fk="tg-msg" spellcheck="true">${esc(msg)}</textarea></div>
      <div class="tg-acts"><button class="btn btn-primary btn-sm grow" data-action="git-commit" data-fk="tg-commit"${n ? '' : ' aria-disabled="true"'}>${ic('check', 'ico-sm')}Commit ${n ? plural(n, 'file') : ''}</button>
        ${VC.moreBtn('git-more', 'More git actions', 'tg-more')}</div>`;
  }
  function emptyHtml() {
    const push = G.ahead ? `<button class="btn btn-primary btn-sm grow" data-action="git-push" data-fk="tg-push">${ic('up', 'ico-sm')}Push ${plural(G.ahead, 'commit')}</button>` : '';
    return `<p class="tg-empty">${ic('check', 'ico-sm')}<span>No changes to commit.${G.ahead ? '' : ' The test files match the last commit.'}</span></p>
      <div class="tg-acts">${push}${push ? '' : '<span class="grow"></span>'}${VC.moreBtn('git-more', 'More git actions', 'tg-more')}</div>`;
  }
  VC.gitPane = () => `<div class="tg">${branchHtml()}${G.files.length ? listHtml() + commitHtml() : emptyHtml()}</div>`;

  /* ---- actions --------------------------------------------------------------- */
  A['git-diff'] = (el) => {
    const i = +el.dataset.i, f = G.files[i];
    VC.diffDialog({
      title: f.path.slice(f.path.lastIndexOf('/') + 1), sub: `${ST[f.st]} · ${f.path} · against ${G.branch}`, file: f.path, isNew: f.st === 'A', lines: f.lines,
      confirm: f.staged ? 'Unstage' : 'Stage',
      after: () => {
        f.staged = !f.staged;
        if (VC.dropWroteToast) VC.dropWroteToast();
        VC.render();
        VC.toast(f.staged ? 'Staged' : 'Unstaged', f.path);
      }
    });
  };
  function commit() {
    const files = staged();
    if (!files.length) { VC.toast('Nothing staged', 'Tick the files to commit', 'warn'); return false; }
    if (!msg.trim()) {
      VC.toast('Write a commit message', 'One line on what the change does', 'warn');
      const t = $('#tg-msg'); if (t) t.focus();
      return false;
    }
    sha = (sha * 31 + 0x9e37) % 0xfffffff;
    const short = sha.toString(16).padStart(7, '0').slice(0, 7);
    G.files = G.files.filter((f) => !f.staged);
    G.ahead += 1;
    G.last = { sha: short, msg: msg.trim(), n: files.length };
    msg = '';
    return short;
  }
  A['git-commit'] = () => {
    const short = commit();
    if (!short) return;
    VC.render();
    VC.toast(`Committed ${short}`, `${G.last.msg} · ${plural(G.last.n, 'file')} · ${plural(G.ahead, 'commit')} to push`, '', { label: 'Push', ico: 'up', action: () => A['git-push']() });
  };
  A['git-push'] = () => {
    if (!G.ahead) { VC.toast('Nothing to push', `${G.branch} matches ${G.upstream}`); return; }
    const n = G.ahead;
    G.ahead = 0;
    VC.render();
    VC.toast(`Pushed ${plural(n, 'commit')}`, `to ${G.upstream}`);
  };
  A['git-pull'] = () => {
    const n = G.behind;
    G.behind = 0;
    VC.render();
    VC.toast(n ? `Pulled ${plural(n, 'commit')}` : 'Already up to date', `from ${G.upstream}`);
  };
  A['git-commit-push'] = () => {
    const short = commit();
    if (!short) return;
    const n = G.ahead;
    G.ahead = 0;
    VC.render();
    VC.toast(`Committed ${short} and pushed`, `${G.last.msg} · ${plural(n, 'commit')} to ${G.upstream}`);
  };
  A['git-copy-branch'] = () => VC.copy(G.branch, 'Copied', G.branch);
  A['git-more'] = (el) => VC.menu(el, `${VC.menuItem('Push', 'git-push', '', { ico: 'up', meta: G.ahead ? `${G.ahead} ahead` : 'up to date', disabled: !G.ahead })}
    ${VC.menuItem('Pull', 'git-pull', '', { ico: 'down', meta: G.behind ? `${G.behind} behind` : 'up to date' })}
    ${VC.menuItem('Commit & push', 'git-commit-push', '', { ico: 'send', disabled: !staged().length })}
    <div class="pop-sep"></div>${VC.menuItem('Copy branch name', 'git-copy-branch', '', { ico: 'copy' })}`);

  /* checkboxes skip the click router, so staging listens for change */
  document.addEventListener('change', (e) => {
    const c = e.target;
    if (c.classList.contains('tg-stage')) G.files[+c.dataset.i].staged = c.checked;
    else if (c.classList.contains('tg-stage-all')) G.files.forEach((f) => { f.staged = c.checked; });
    else return;
    VC.render();
    const f = $(`[data-fk="${CSS.escape(c.dataset.fk)}"]`); if (f) f.focus({ preventScroll: true });
  });
  document.addEventListener('input', (e) => { if (e.target.id === 'tg-msg') msg = e.target.value; });
})();
