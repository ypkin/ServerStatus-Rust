// Fork overlay: show each node's plan on the main panel without rebuilding
// the precompiled React bundle.
//
// The React table renders one row per server; the first cell holds the
// display name (alias, falling back to the node name) inside a truncated
// div. This script appends one small plan line to that cell, and re-injects
// it after React re-renders (MutationObserver), because React wipes
// foreign DOM on every stats refresh.
//
// Data comes from /json/stats.json (public): plan_start_date, annual_fee,
// bandwidth_mbps, traffic_limit. Unset items are omitted, never fabricated
// (in particular no expiry date is derived). If every plan item is unset
// the line reads "套餐：—" so the row visibly exists but claims nothing.
(function () {
  'use strict';
  var GiB = 1024 * 1024 * 1024;
  var planByName = {};

  function fmtGiB(bytes) {
    var v = bytes / GiB;
    return (Math.round(v * 100) / 100) + 'GiB';
  }

  function fmtFee(fee) {
    return '$' + (Math.round(fee * 100) / 100);
  }

  function planLine(s) {
    var parts = [];
    if (s.plan_start_date) parts.push('开始 ' + s.plan_start_date);
    if (s.annual_fee > 0) parts.push('年费 ' + fmtFee(s.annual_fee));
    if (s.bandwidth_mbps > 0) parts.push('带宽 ' + s.bandwidth_mbps + 'Mbps');
    if (s.traffic_limit > 0) parts.push('限额 ' + fmtGiB(s.traffic_limit));
    else if (parts.length > 0) parts.push('限额不限量');
    return parts.length ? '套餐：' + parts.join(' · ') : '套餐：—';
  }

  function specLine(s) {
    // Static specs for the same row (see client auto-detect): CPU model +
    // cores, memory total, disk total, OS/kernel, arch. All come from
    // stats.json (sys_info + top-level totals); anything missing is omitted.
    var parts = [];
    var si = s.sys_info || {};
    if (si.cpu_brand && si.cpu_num) parts.push(si.cpu_brand + ' × ' + si.cpu_num);
    else if (si.cpu_brand) parts.push(si.cpu_brand);
    if (s.memory_total > 0) parts.push('内存 ' + fmtGiB(s.memory_total));
    if (s.hdd_total > 0) parts.push('硬盘 ' + fmtGiB(s.hdd_total));
    var os = [si.os_name, si.os_release, si.kernel_version].filter(Boolean).join(' ');
    if (os) parts.push(os);
    else if (si.os_name) parts.push(si.os_name);
    if (si.os_arch) parts.push(si.os_arch);
    return parts.length ? '配置：' + parts.join(' · ') : '';
  }

  function probeLines(srv) {
    // One latency line per user-configured probe (see stats.json
    // ping_probes): only configured probes/stacks appear, under the
    // user-edited probe name. A configured stack with no measurement yet
    // shows —. Unconfigured probes never appear here.
    var lines = [];
    (srv.ping_probes || []).forEach(function (p) {
      var stack = function (v) {
        if (!v) return null;
        return (v.time > 0 || v.loss > 0) ? (v.time + 'ms/' + v.loss + '%') : '—';
      };
      var parts = [];
      var v4 = stack(p.v4), v6 = stack(p.v6);
      if (v4 !== null) parts.push('v4 ' + v4);
      if (v6 !== null) parts.push('v6 ' + v6);
      if (parts.length) lines.push('延迟 ' + p.name + '：' + parts.join(' · '));
    });
    return lines;
  }

  function inject() {
    var names = Object.keys(planByName);
    if (!names.length) return;
    // Anchor: the alias cell div (class "truncate" inside a td).
    var cells = document.querySelectorAll('td div.truncate');
    var handled = new Set();
    cells.forEach(function (div) {
      var text = (div.textContent || '').trim();
      var srv = null;
      for (var i = 0; i < names.length; i++) {
        var s = planByName[names[i]];
        if (text === s.display) { srv = s; break; }
      }
      if (!srv) return;
      var td = div.closest('td');
      if (!td || handled.has(td)) return;
      handled.add(td);
      // Desired overlay lines for this row, in order.
      var wanted = [planLine(srv)];
      var spec = specLine(srv);
      if (spec) wanted.push(spec);
      probeLines(srv).forEach(function (l) { wanted.push(l); });
      // Loop-safe dedupe: only touch the DOM when the current overlay
      // text differs from what we want (our own mutations re-trigger the
      // observer, so an unconditional rewrite would spin forever).
      var existing = Array.prototype.slice.call(td.querySelectorAll('[data-ssr-kind]'));
      var same = existing.length === wanted.length
        && existing.every(function (el, i) { return el.textContent === wanted[i]; });
      if (same) return;
      existing.forEach(function (el) { el.remove(); });
      wanted.forEach(function (text, i) {
        var line = document.createElement('div');
        line.setAttribute('data-ssr-kind', i === 0 ? 'plan' : 'line');
        line.style.cssText = 'font-size:11px;line-height:1.5;color:#6b7280;white-space:normal;word-break:break-word;margin-top:2px';
        line.textContent = text;
        td.appendChild(line);
      });
    });
  }

  function refresh() {
    fetch('/json/stats.json', { cache: 'no-store' })
      .then(function (r) { return r.json(); })
      .then(function (j) {
        var map = {};
        (j.servers || []).forEach(function (s) {
          map[s.name] = {
            display: (s.alias || s.name || '').trim(),
            plan_start_date: s.plan_start_date || '',
            annual_fee: s.annual_fee || 0,
            bandwidth_mbps: s.bandwidth_mbps || 0,
            traffic_limit: s.traffic_limit || 0,
            memory_total: s.memory_total || 0,
            hdd_total: s.hdd_total || 0,
            sys_info: s.sys_info || null,
            ping_probes: s.ping_probes || [],
          };
        });
        planByName = map;
        inject();
      })
      .catch(function () { /* keep last map; retry next interval */ });
  }

  var timer = null;
  function scheduleInject() {
    if (timer) return;
    timer = setTimeout(function () { timer = null; inject(); }, 300);
  }

  document.addEventListener('DOMContentLoaded', function () {
    refresh();
    setInterval(refresh, 30000);
    new MutationObserver(scheduleInject).observe(document.body, { childList: true, subtree: true });
    setTimeout(inject, 1500);
    setTimeout(inject, 5000);
  });
})();
