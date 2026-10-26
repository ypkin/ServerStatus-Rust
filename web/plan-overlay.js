// Fork overlay: group pills + expandable node details on the main panel
// without rebuilding the precompiled React bundle.
//
// The React table renders one row per server; the first cell holds the
// display name (alias, falling back to the node name) inside a truncated
// div. By default a row shows only its React summary. Clicking the name
// cell expands a nezha-style detail block under the name (plan, system,
// CPU, disk, memory, swap, traffic, load, processes, connections, boot
// time, uptime, probes, public note); clicking again collapses it.
// A nezha-style pill bar above the panel groups rows by Tag (empty Tag =
// 默认组): dark rounded pills, white bold text, "name count"; clicking a
// pill filters rows to that Tag, clicking it again (or 全部) clears.
// React wipes foreign DOM on every stats refresh, so a MutationObserver
// re-applies indicators, detail blocks and the row filter (loop-safe:
// only touch the DOM when the wanted content differs). The pill bar
// itself lives before #root, outside React's tree, so React never wipes
// it; it is rebuilt only when its signature (counts/active) changes.
//
// Data comes from /json/stats.json (public). Anything missing is omitted,
// never fabricated (in particular no expiry date is derived).
(function () {
  'use strict';
  var GiB = 1024 * 1024 * 1024;
  var planByName = {};
  var expanded = {}; // node name -> true while its detail block is open
  var activeTag = null; // null = all rows; '' = default group; else a Tag
  var haveStats = false; // true once stats.json has loaded at least once

  function tagDisplay(tag) { return tag ? tag : '默认'; }

  function pillsData() {
    var counts = {};
    Object.keys(planByName).forEach(function (n) {
      var t = planByName[n].tag || '';
      counts[t] = (counts[t] || 0) + 1;
    });
    return counts;
  }

  function renderPills() {
    if (!haveStats) return;
    var counts = pillsData();
    var tags = Object.keys(counts).sort(function (a, b) {
      if (a === '') return -1;
      if (b === '') return 1;
      return a < b ? -1 : (a > b ? 1 : 0);
    });
    var total = 0;
    tags.forEach(function (t) { total += counts[t]; });
    var sigParts = ['all:' + total, 'active:' + (activeTag === null ? '*' : activeTag)];
    tags.forEach(function (t) { sigParts.push(t + '=' + counts[t]); });
    var sig = sigParts.join('|');
    var bar = document.getElementById('ssr-tag-pills');
    if (bar && bar.getAttribute('data-ssr-sig') === sig) return;
    if (!bar) {
      bar = document.createElement('div');
      bar.id = 'ssr-tag-pills';
      var root = document.getElementById('root');
      if (root && root.parentNode) root.parentNode.insertBefore(bar, root);
      else document.body.insertBefore(bar, document.body.firstChild);
    }
    bar.setAttribute('data-ssr-sig', sig);
    bar.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px;padding:12px 16px;align-items:center;';
    while (bar.firstChild) bar.removeChild(bar.firstChild);
    var mk = function (label, tag) {
      var b = document.createElement('button');
      var isActive = (tag === null ? activeTag === null : activeTag === tag);
      b.style.cssText = 'border:0;border-radius:999px;padding:6px 14px;font-size:13px;font-weight:700;color:#fff;cursor:pointer;background:' + (isActive ? '#0a6cff' : '#374151');
      b.textContent = label;
      b.title = tag === null ? '显示全部节点' : '只显示 ' + tagDisplay(tag) + ' 组的节点';
      b.addEventListener('click', function () {
        if (tag === null) activeTag = null;
        else if (activeTag === tag) activeTag = null;
        else activeTag = tag;
        renderPills();
        inject();
      });
      return b;
    };
    bar.appendChild(mk('全部 ' + total, null));
    tags.forEach(function (t) {
      bar.appendChild(mk(tagDisplay(t) + ' ' + counts[t], t));
    });
  }

  function fmtGiB(bytes) {
    var v = bytes / GiB;
    return (Math.round(v * 100) / 100) + 'GiB';
  }

  // Units in stats.json (see client sys_info::sample): memory totals are
  // KiB (bytes / 1024), disk totals are MB (calc_hdd_stats). The detail
  // view divides by the same units, so these helpers must too — dividing
  // by 2^30 as if they were bytes yields 0GiB for every real machine.
  function fmtGiBFromKiB(kib) {
    return (Math.round(kib / 1024 / 1024 * 100) / 100) + 'GiB';
  }
  function fmtGiBFromMB(mb) {
    return (Math.round(mb / 1024 * 100) / 100) + 'GiB';
  }

  function fmtFee(fee, cur) {
    return (cur || '$') + (Math.round(fee * 100) / 100);
  }

  // Bandwidth display, same rule as the admin table (fmtBw there):
  // storage stays Mbps; <1000 -> Mbps, >=1000 -> Gbps, >=1000000 -> Tbps.
  function fmtBw(mbps) {
    mbps = Number(mbps) || 0;
    if (mbps >= 1000000) return trimNum(mbps / 1000000) + 'Tbps';
    if (mbps >= 1000) return trimNum(mbps / 1000) + 'Gbps';
    return mbps + 'Mbps';
  }
  function trimNum(v) {
    return String(Math.round(v * 1000) / 1000);
  }

  function fmtDate(ts) {
    if (!ts) return '—';
    try { return new Date(ts * 1000).toLocaleString(); } catch (e) { return '—'; }
  }

  function pct(used, total) {
    if (!total) return '';
    return ' (' + (Math.round(used / total * 1000) / 10) + '%)';
  }

  function planLine(s) {
    var parts = [];
    if (s.plan_start_date) parts.push('开始 ' + s.plan_start_date);
    if (s.annual_fee > 0) parts.push('年费 ' + fmtFee(s.annual_fee, s.currency));
    if (s.bandwidth_mbps > 0) parts.push('带宽 ' + fmtBw(s.bandwidth_mbps));
    if (s.traffic_limit > 0) parts.push('限额 ' + fmtGiB(s.traffic_limit));
    else if (parts.length > 0) parts.push('限额不限量');
    return parts.length ? '套餐：' + parts.join(' · ') : '套餐：—';
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

  function bootLine(uptimeSec) {
    if (!uptimeSec || uptimeSec <= 0) return '';
    var d = new Date(Date.now() - uptimeSec * 1000);
    var p = function (n) { return (n < 10 ? '0' : '') + n; };
    return '启动时间：' + d.getFullYear() + '-' + p(d.getMonth() + 1) + '-' + p(d.getDate())
      + ' ' + p(d.getHours()) + ':' + p(d.getMinutes());
  }

  function detailLines(s) {
    var lines = [];
    var si = s.sys_info || {};
    // Plan line plus live IPv4/IPv6 badges (batch 3: the expanded area
    // must carry the plan + connectivity, not just the collapsed row).
    lines.push(planLine(s) + ' · IPv4 ' + (s.online4 ? '✓' : '✗') + ' · IPv6 ' + (s.online6 ? '✓' : '✗'));
    var os = [si.os_name, si.os_release, si.kernel_version].filter(Boolean).join(' ');
    var sysParts = [];
    if (os) sysParts.push(os);
    if (si.os_arch) sysParts.push(si.os_arch);
    if (s.host_type) sysParts.push(s.host_type);
    if (sysParts.length) lines.push('系统：' + sysParts.join(' · '));
    if (si.cpu_brand || si.cpu_num) {
      var cpu = (si.cpu_brand || '') + (si.cpu_num ? ' × ' + si.cpu_num : '');
      if (typeof s.cpu === 'number') cpu += ' · 占用 ' + (Math.round(s.cpu * 10) / 10) + '%';
      lines.push('CPU：' + cpu);
    }
    if (s.hdd_total > 0) {
      lines.push('硬盘：' + fmtGiBFromMB(s.hdd_used) + ' / ' + fmtGiBFromMB(s.hdd_total) + pct(s.hdd_used, s.hdd_total));
    }
    if (s.memory_total > 0) {
      lines.push('内存：' + fmtGiBFromKiB(s.memory_used) + ' / ' + fmtGiBFromKiB(s.memory_total) + pct(s.memory_used, s.memory_total));
    }
    if (s.swap_total > 0) {
      lines.push('交换：' + fmtGiBFromKiB(s.swap_used) + ' / ' + fmtGiBFromKiB(s.swap_total) + pct(s.swap_used, s.swap_total));
    }
    var tParts = [];
    if (s.month_rx + s.month_tx > 0) {
      tParts.push('本月入 ' + fmtGiB(s.month_rx) + ' / 出 ' + fmtGiB(s.month_tx));
    }
    tParts.push('累计入 ' + fmtGiB(s.total_rx) + ' / 出 ' + fmtGiB(s.total_tx));
    tParts.push('本月已用 ' + fmtGiB(s.month_used));
    tParts.push('总量 ' + fmtGiB(s.total_rx + s.total_tx));
    tParts.push(s.traffic_limit > 0 ? '限额 ' + fmtGiB(s.traffic_limit) : '限额不限量');
    lines.push('流量：' + tParts.join(' · '));
    lines.push('负载：' + s.load_1 + ' / ' + s.load_5 + ' / ' + s.load_15);
    lines.push('进程：' + s.process_count);
    lines.push('连接：TCP ' + s.tcp_count + ' / UDP ' + s.udp_count);
    var boot = bootLine(s.uptime);
    if (boot) lines.push(boot);
    if (s.uptime_str) lines.push('在线时长：' + s.uptime_str);
    lines.push('最后活动：' + fmtDate(s.latest_ts));
    if (s.version) lines.push('版本号：' + s.version);
    probeLines(s).forEach(function (l) { lines.push(l); });
    if (s.public_note) lines.push('公开备注：' + s.public_note);
    return lines;
  }

  function inject() {
    var names = Object.keys(planByName);
    if (!names.length) return;
    renderPills();
    // Anchor: the alias cell div (class "truncate" inside a td).
    var cells = document.querySelectorAll('td div.truncate');
    var handled = new Set();
    cells.forEach(function (div) {
      // Strip our own indicator before matching: after the first inject
      // the div text starts with ▸/▾, which is not part of the name.
      var text = (div.textContent || '').trim().replace(/^[▸▾]/, '').trim();
      var srv = null;
      for (var i = 0; i < names.length; i++) {
        var s = planByName[names[i]];
        if (text === s.display) { srv = s; break; }
      }
      if (!srv) return;
      var td = div.closest('td');
      if (!td || handled.has(td)) return;
      handled.add(td);
      // Drop lines injected by the pre-fold overlay version, if any.
      Array.prototype.slice.call(td.querySelectorAll('[data-ssr-kind]')).forEach(function (el) { el.remove(); });
      // Expand/collapse indicator at the head of the name cell.
      var ind = div.querySelector('[data-ssr-exp]');
      if (!ind) {
        ind = document.createElement('span');
        ind.setAttribute('data-ssr-exp', '1');
        ind.style.cssText = 'cursor:pointer;margin-right:4px;';
        div.insertBefore(ind, div.firstChild);
      }
      var wantInd = expanded[srv.name] ? '▾' : '▸';
      if (ind.textContent !== wantInd) ind.textContent = wantInd;
      // Tag filter: hide rows outside the active pill's Tag. Unmapped
      // rows (name not found in stats) always stay visible.
      var tr = td.closest('tr');
      if (tr) {
        var wantDisp = (activeTag !== null && (srv.tag || '') !== activeTag) ? 'none' : '';
        if (tr.style.display !== wantDisp) tr.style.display = wantDisp;
      }
      // Detail block: present iff expanded, rebuilt only when changed.
      var block = td.querySelector('[data-ssr-detail]');
      if (!expanded[srv.name]) {
        if (block) block.remove();
        return;
      }
      var wanted = detailLines(srv);
      var key = wanted.join('\n');
      if (block && block.getAttribute('data-ssr-key') === key) return;
      if (block) block.remove();
      block = document.createElement('div');
      block.setAttribute('data-ssr-detail', '1');
      block.setAttribute('data-ssr-key', key);
      wanted.forEach(function (text) {
        var line = document.createElement('div');
        line.style.cssText = 'font-size:11px;line-height:1.5;color:#6b7280;white-space:normal;word-break:break-word;margin-top:2px';
        line.textContent = text;
        block.appendChild(line);
      });
      td.appendChild(block);
    });
  }

  function onDocClick(e) {
    var t = e.target && e.target.closest ? e.target.closest('div.truncate') : null;
    if (!t || !t.closest('td')) return;
    var text = (t.textContent || '').trim().replace(/^[▸▾]/, '').trim();
    var srv = null;
    var names = Object.keys(planByName);
    for (var i = 0; i < names.length; i++) {
      var s = planByName[names[i]];
      if (text === s.display) { srv = s; break; }
    }
    if (!srv) return;
    // Deliberately no stopPropagation/preventDefault: React's own row
    // click behaviour (if any) must keep working.
    if (expanded[srv.name]) delete expanded[srv.name];
    else expanded[srv.name] = true;
    inject();
  }

  function refresh() {
    fetch('/json/stats.json', { cache: 'no-store' })
      .then(function (r) { return r.json(); })
      .then(function (j) {
        var map = {};
        (j.servers || []).forEach(function (s) {
          map[s.name] = {
            name: s.name,
            tag: s.tag || '',
            display: (s.alias || s.name || '').trim(),
            plan_start_date: s.plan_start_date || '',
            annual_fee: s.annual_fee || 0,
            currency: s.currency || '$',
            bandwidth_mbps: s.bandwidth_mbps || 0,
            traffic_limit: s.traffic_limit || 0,
            host_type: s.host_type || '',
            online4: !!s.online4,
            online6: !!s.online6,
            latest_ts: s.latest_ts || 0,
            version: s.version || '',
            cpu: (typeof s.cpu === 'number') ? s.cpu : null,
            memory_total: s.memory_total || 0,
            memory_used: s.memory_used || 0,
            swap_total: s.swap_total || 0,
            swap_used: s.swap_used || 0,
            hdd_total: s.hdd_total || 0,
            hdd_used: s.hdd_used || 0,
            month_rx: s.month_rx || 0,
            month_tx: s.month_tx || 0,
            total_rx: s.total_rx || 0,
            total_tx: s.total_tx || 0,
            month_used: s.month_used || 0,
            load_1: s.load_1 || 0,
            load_5: s.load_5 || 0,
            load_15: s.load_15 || 0,
            process_count: s.process_count || 0,
            tcp_count: s.tcp_count || 0,
            udp_count: s.udp_count || 0,
            uptime: s.uptime || 0,
            uptime_str: s.uptime_str || '',
            public_note: s.public_note || '',
            sys_info: s.sys_info || null,
            ping_probes: s.ping_probes || [],
          };
        });
        planByName = map;
        haveStats = true;
        renderPills();
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
    document.addEventListener('click', onDocClick);
    new MutationObserver(scheduleInject).observe(document.body, { childList: true, subtree: true });
    setTimeout(inject, 1500);
    setTimeout(inject, 5000);
  });
})();
