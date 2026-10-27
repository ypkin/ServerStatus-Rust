// Fork overlay: group pills + expandable node details on the main panel
// without rebuilding the precompiled React bundle.
//
// The React table renders one row per server; the first cell holds the
// display name (alias, falling back to the node name) inside a truncated
// div. Clicking any cell of the row expands React's own detail row
// under it (bundle: tr onClick); the overlay appends a tabbed detail
// block inside that wide row while hiding React's own centered
// detail block there (the user
// asked to delete the native load/memory/swap/disk/TCP/UDP/latency
// block and keep only the overlay block): a
// node summary line, then 网络延迟 / 系统概览 tabs. The latency tab
// carries the live probe lines plus a Prism-ported chart (1h/6h/24h/7d
// ranges, 削峰/平滑 toggles, one line per user-named probe, v4/v6
// split); the chart is sampled by the browser itself from the live
// stats stream (one value per probe/stack per 30 s refresh), so
// closing the browser restarts its history. The overview tab carries
// only what React's own expanded row does not already show (plan,
// system, CPU model, traffic, boot time, online duration, last
// activity, version, public note); clicking again collapses it. The
// 节点/协议 columns are swapped (protocol first) and the Tag pill bar
// sits in the page header next to the ServerStatus title.
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

  // Batch 3: the pill bar lives in the page header, next to the
  // ServerStatus title (h1.logo), not above the table. The title is
  // shrunk so title + pills + theme toggle fit one row without
  // overlapping; on narrow screens the header wraps. React only
  // re-renders the header on a theme toggle, which would wipe a bar
  // placed inside it — so placement is re-checked on every renderPills
  // call, not just when the pill signature changes. Before React has
  // rendered the header, the bar falls back to sitting before #root
  // and is moved up on the next inject.
  function placePills(bar) {
    var h1 = document.querySelector('h1.logo');
    if (h1 && h1.parentNode) {
      h1.style.fontSize = '18px';
      h1.style.whiteSpace = 'nowrap';
      var cont = h1.parentNode;
      cont.style.flexWrap = 'wrap';
      if (bar.parentNode !== cont || bar.previousSibling !== h1) {
        cont.insertBefore(bar, h1.nextSibling);
      }
      bar.style.padding = '4px 8px';
      bar.style.flex = '1 1 auto';
      bar.style.justifyContent = 'flex-start';
      return;
    }
    var root = document.getElementById('root');
    if (root && root.parentNode && bar.parentNode !== root.parentNode) {
      root.parentNode.insertBefore(bar, root);
    }
    bar.style.padding = '12px 16px';
    bar.style.flex = '';
  }

  // Mobile header fix (2026-10-27): the React header wrapper #header is
  // a fixed 80px tall on narrow screens (bundle: h-[80px] ... md:h-[100px]),
  // while the pill bar injected above makes the header row taller once it
  // wraps. #body below it is opaque (dark:bg-slate-700) and pulled 20px up
  // (-mt-[20px]), so the wrapped pills + theme toggle get painted over and
  // vanish on phones (only the logo stays visible). Below the Tailwind md
  // breakpoint (768px) let the header grow, keep 28px clearance above the
  // body's overlap, and put the pills on their own full-width row under
  // logo + toggle. Pure CSS media query: immune to React re-renders and
  // to rotation/resize, and it never applies at >=768px, so the desktop
  // one-row header is byte-for-byte untouched.
  function ensureHeaderFixStyle() {
    if (document.getElementById('ssr-mobile-header-fix')) return;
    var st = document.createElement('style');
    st.id = 'ssr-mobile-header-fix';
    st.textContent = '@media (max-width: 767px){'
      + '#header{height:auto !important;padding-bottom:28px !important;}'
      + '#header > .container{flex-wrap:wrap !important;}'
      + '#header h1.logo{order:1;}'
      + '#header > .container > div.pr-8{order:2;}'
      + '#ssr-tag-pills{order:3;flex:1 1 100% !important;}'
      + '}';
    document.head.appendChild(st);
  }

  // Dark-mode detail text (2026-10-27): the overlay detail block's
  // text colours are hardcoded light-mode inline styles (#111827
  // summary, #6b7280 lines, #374151 inactive buttons), so in dark
  // mode they sit dark-on-dark and are unreadable (phone screenshot:
  // summary/overview/probe lines and inactive tabs all but invisible,
  // while the native table rows are white and fine). Inline styles
  // beat stylesheets, hence !important. Scoped strictly to
  // [data-ssr-detail] descendants; the site's dark mode is the
  // Tailwind `.dark` class on <html> (the bundle toggles
  // documentElement), so these rules never apply in light mode and
  // the light appearance is byte-for-byte unchanged. The tooltip chip
  // keeps its own dark bg + white text (excluded via its fixed
  // position) and the chart svg keeps its own light card (#f9fafb bg,
  // #9ca3af axis labels), readable as-is, so it is left alone here;
  // the chart card's dark colours live separately in
  // ensureChartDarkStyle (v21: v16 deliberately left the chart light).
  function ensureDetailDarkStyle() {
    if (document.getElementById('ssr-detail-dark')) return;
    var st = document.createElement('style');
    st.id = 'ssr-detail-dark';
    st.textContent =
      '.dark [data-ssr-detail]{color:#f9fafb !important;}'
      + '.dark [data-ssr-detail] div:not([style*="position:fixed"]){color:#d1d5db !important;}'
      + '.dark [data-ssr-detail] > div:first-child{color:#f9fafb !important;}'
      + '.dark [data-ssr-detail] button{color:#e5e7eb !important;}';
    document.head.appendChild(st);
  }

  // Dark-mode chart card (2026-10-27, v21): the latency chart svg
  // carries its own light card (inline background #f9fafb, #e5e7eb
  // grid, #9ca3af ticks), so in dark mode it stayed a white card
  // under dark detail text. These rules are scoped strictly to
  // .dark + the chart svg marker (data-ssr-chart, data-ssr-grid),
  // so light mode is byte-for-byte unchanged: curves keep their
  // colours, the tooltip keeps its dark chip, the vline keeps
  // #6b7280 (visible on dark). Inline background beats stylesheets,
  // hence !important on the card rule; grid/tick rules override
  // presentation attributes, which lose to any stylesheet rule.
  function ensureChartDarkStyle() {
    if (document.getElementById('ssr-chart-dark')) return;
    var st = document.createElement('style');
    st.id = 'ssr-chart-dark';
    st.textContent =
      '.dark svg[data-ssr-chart]{background:#1e293b !important;}'
      + '.dark svg[data-ssr-chart] line[data-ssr-grid]{stroke:#374151 !important;}'
      + '.dark svg[data-ssr-chart] text{fill:#d1d5db !important;}';
    document.head.appendChild(st);
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
    if (bar && bar.getAttribute('data-ssr-sig') === sig) {
      placePills(bar);
      return;
    }
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
    placePills(bar);
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

  // Unlimited traffic spellings: 0 (old) and 9999 GiB (admin form).
  // 9999 must never render as a quota (see traffic::is_unlimited).
  function isUnlimitedTraffic(bytes) {
    return !bytes || Math.round(bytes / GiB) === 9999;
  }

  function planLine(s) {
    var parts = [];
    if (s.plan_start_date) parts.push('开始 ' + s.plan_start_date);
    // Batch 3: end date shows only when the user actually filled it.
    // Nothing is ever derived from the fee or the start date.
    if (s.plan_end_date) parts.push('结束 ' + s.plan_end_date);
    if (s.annual_fee > 0) parts.push('年费 ' + fmtFee(s.annual_fee, s.currency));
    if (s.bandwidth_mbps > 0) parts.push('带宽 ' + fmtBw(s.bandwidth_mbps));
    if (isUnlimitedTraffic(s.traffic_limit)) {
      if (parts.length > 0) parts.push('限额不限量');
    } else parts.push('限额 ' + fmtGiB(s.traffic_limit));
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

  // Boot time from the real seconds field (stats uptime_sec). The old
  // code read stats "uptime", which is the formatted string ("N 天" /
  // "HH:MM:SS") — multiplying a string gave NaN and the panel printed
  // "NaN-NaN-NaN NaN:NaN". Any non-numeric, non-positive or
  // out-of-range value hides the row instead of printing NaN.
  function bootLine(uptimeSec) {
    var sec = Number(uptimeSec);
    if (!isFinite(sec) || sec <= 0) return '';
    var d = new Date(Date.now() - sec * 1000);
    if (isNaN(d.getTime())) return '';
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
    // Batch 3 (cdt detail fix): the overlay detail keeps ONLY what
    // React does not already show — plan, system, CPU model, traffic,
    // boot, online duration, last activity, version, public note.
    // Load, memory, disk, swap, process/connection counts and probe
    // latency already live in React's own expanded row below, so
    // repeating them here stacked two identical sections together.
    if (si.cpu_brand || si.cpu_num) {
      var cpu = (si.cpu_brand || '') + (si.cpu_num ? ' × ' + si.cpu_num : '');
      lines.push('CPU：' + cpu);
    }
    var tParts = [];
    tParts.push('已用 ' + fmtGiB(s.month_used));
    // Monthly allowance block: used is month_used (the billing-mode
    // figure the admin correction form sets — never rx+tx); 9999 =
    // unlimited; no limit set = dash, never a fabricated number;
    // over limit shows the overage.
    var tLim = s.traffic_limit;
    if (!tLim) {
      tParts.push('剩余 —');
      tParts.push('每月限制 —');
    } else if (Math.round(tLim / GiB) === 9999) {
      tParts.push('剩余不限量');
      tParts.push('每月限制不限量');
    } else {
      var tRem = tLim - (s.month_used || 0);
      tParts.push(tRem >= 0 ? '剩余 ' + fmtGiB(tRem) : '已超 ' + fmtGiB(-tRem));
      tParts.push('每月限制 ' + fmtGiB(tLim));
    }
    lines.push('本月流量：' + tParts.join(' · '));
    var boot = bootLine(s.uptime_sec);
    if (boot) lines.push(boot);
    if (s.uptime_display) lines.push('在线时长：' + s.uptime_display);
    lines.push('最后活动：' + fmtDate(s.latest_ts));
    if (s.version) lines.push('版本号：' + s.version);
    if (s.public_note) lines.push('公开备注：' + s.public_note);
    return lines;
  }

  // --- Latency chart (batch 5, ported from Prism's detail drawer) ---
  // The chart reads server-side probe history (see ping_hist.rs):
  // every client report is bucketed by the server into minute
  // buckets (1h/6h/24h ranges) and hour buckets (7d), persisted in
  // ping_history.json, so expanding a node draws its curve at once
  // instead of waiting for the browser to sample points. History
  // only exists from the deploy that introduced it; nothing older
  // is backfilled or fabricated. Between fetches, the live tail
  // below appends the current realtime value on each stats refresh
  // (minute ranges only: mixing second-resolution tail points into
  // the 7d hour series would shatter its line segments), and the
  // next fetch replaces the tail with the server's own buckets.
  // Nothing is fabricated: a timeout (the client caps at 1000 ms, so
  // >= 1000 means the attempt timed out) is a null gap, and an
  // unmeasured stack contributes no point at all. Tooltip shows raw
  // bucket values, never the despiked/smoothed line (Prism's rule).
  var latSrv = {}; // 'name|rangeHours' -> {slot: [[ts, ms]...]} fetched from /api/ping-history
  var latSrvState = {}; // 'name|rangeHours' -> 'loading' | 'ready' | 'error'
  var latLive = {}; // name -> key -> [[ts, ms]] live tail from stats refreshes
  var latLiveTs = {}; // name -> ts of last live append (detail re-render key)
  var detailState = {}; // name -> {tab, range, despike, smooth}

  function detailStateFor(name) {
    if (!detailState[name]) detailState[name] = { tab: 'latency', range: 24, despike: false, smooth: true };
    return detailState[name];
  }

  function latVal(v) {
    if (!v) return undefined; // unmeasured stack: no point, not a gap
    if (v.time >= 1000 || v.loss >= 100) return null; // capped timeout
    return v.time > 0 ? v.time : undefined;
  }
  function latRangeParam(rangeHours) {
    return rangeHours >= 168 ? '7d' : rangeHours <= 1 ? '1h' : rangeHours <= 6 ? '6h' : '24h';
  }
  function latSrvKey(name, rangeHours) { return name + '|' + rangeHours; }
  // Fetch server history for one node's range, then re-render the
  // detail (the inject key gains a hist= segment, so the block
  // redraws once the fetch lands). Deduped: a second call while
  // loading or after success is a no-op; on error the chart shows
  // a retry button instead of retrying in a loop.
  function fetchLatHistory(name, rangeHours) {
    var key = latSrvKey(name, rangeHours);
    if (latSrvState[key] === 'loading' || latSrvState[key] === 'ready') return;
    latSrvState[key] = 'loading';
    fetch('/api/ping-history?host=' + encodeURIComponent(name) + '&range=' + latRangeParam(rangeHours), { cache: 'no-store' })
      .then(function (r) { return r.json(); })
      .then(function (j) {
        var bySlot = {};
        (j.probes || []).forEach(function (pr) {
          bySlot[pr.slot] = (pr.points || []).map(function (pt) { return [pt[0], pt[1]]; });
        });
        latSrv[key] = bySlot;
        latSrvState[key] = 'ready';
        inject();
      })
      .catch(function () {
        latSrvState[key] = 'error';
        inject();
      });
  }
  // Live tail: append the current realtime probe values for an
  // expanded node on each stats refresh. Points at or before the
  // last server point are dropped when the series is built, so a
  // later fetch never double-draws them.
  function appendLatLive(srv, nowSec) {
    var any = false;
    (srv.ping_probes || []).forEach(function (p) {
      ['v4', 'v6'].forEach(function (stack) {
        var val = latVal(p[stack]);
        if (val === undefined) return;
        var key = p.name + ' ' + stack;
        if (!latLive[srv.name]) latLive[srv.name] = {};
        if (!latLive[srv.name][key]) latLive[srv.name][key] = [];
        var tail = latLive[srv.name][key];
        if (!tail.length || tail[tail.length - 1][0] !== nowSec) tail.push([nowSec, val]);
        while (tail.length > 240) tail.shift();
        any = true;
      });
    });
    if (any) latLiveTs[srv.name] = nowSec;
  }
  function latSeries(name, rangeHours, probes) {
    // Series are built from the CONFIGURED probes (stats
    // ping_probes), joined against fetched history by slot, so an
    // unconfigured slot's stale series is never drawn and a
    // configured probe with no history yet still yields an empty
    // series (the chart's empty state, not a missing line). Display
    // keys keep the current probe name, so a rename keeps one
    // continuous series under the new name.
    var srv = latSrv[latSrvKey(name, rangeHours)] || {};
    var out = [];
    (probes || []).forEach(function (p) {
      ['v4', 'v6'].forEach(function (stack) {
        if (!p[stack]) return;
        var dispKey = p.name + ' ' + stack;
        var pts = [];
        if (p.slot) {
          pts = (srv[p.slot + ':' + stack] || []).slice();
        }
        // Live tail on minute ranges only (see header comment).
        if (rangeHours < 168) {
          var live = (latLive[name] && latLive[name][dispKey]) || [];
          live.forEach(function (lp) {
            if (!pts.length || lp[0] > pts[pts.length - 1][0]) pts.push(lp);
          });
        }
        // Downsample to at most 360 points (Prism merges the same way).
        if (pts.length > 360) {
          var group = Math.ceil(pts.length / 360), merged = [];
          for (var i = 0; i < pts.length; i += group) {
            var slice = pts.slice(i, i + group), sum = 0, n = 0;
            slice.forEach(function (pp) { if (pp[1] !== null) { sum += pp[1]; n++; } });
            merged.push([slice[0][0], n > 0 ? Math.round(sum / n) : null]);
          }
          pts = merged;
        }
        out.push({ key: dispKey, points: pts });
      });
    });
    return out;
  }

  // Prism's probe colours (v4/v6 of one carrier differ), fallback palette
  // for user-named probes.
  var LAT_COLORS = {
    unicom: { v4: '#38BDF8', v6: '#C084FC' },
    mobile: { v4: '#FA471E', v6: '#2BEA5E' },
    telecom: { v4: '#FACC15', v6: '#F472B6' },
  };
  var LAT_FALLBACK = ['#38BDF8', '#FA471E', '#FACC15', '#2BEA5E', '#C084FC', '#F472B6'];
  function latColor(key, idx) {
    var low = key.toLowerCase();
    var stack = low.indexOf(' v6') >= 0 ? 'v6' : 'v4';
    if (low.indexOf('联通') >= 0 || low.indexOf('unicom') >= 0) return LAT_COLORS.unicom[stack];
    if (low.indexOf('移动') >= 0 || low.indexOf('mobile') >= 0) return LAT_COLORS.mobile[stack];
    if (low.indexOf('电信') >= 0 || low.indexOf('telecom') >= 0) return LAT_COLORS.telecom[stack];
    return LAT_FALLBACK[idx % LAT_FALLBACK.length];
  }

  // Despike: port of Prism format.ts despike (window 7, 3 sigma MAD).
  function latMedian(vals) {
    var s = vals.slice().sort(function (a, b) { return a - b; });
    var h = s.length >> 1;
    return s.length % 2 ? s[h] : (s[h - 1] + s[h]) / 2;
  }
  function latDespike(values, window, sigmas) {
    window = window || 7; sigmas = sigmas || 3;
    var half = window >> 1;
    return values.map(function (v, i) {
      if (v === null) return v;
      var near = values.slice(Math.max(0, i - half), i + half + 1).filter(function (n) { return n !== null; });
      if (!near.length) return v;
      var mid = latMedian(near);
      var mad = Math.max(latMedian(near.map(function (n) { return Math.abs(n - mid); })), 1);
      return Math.abs(v - mid) > sigmas * 1.4826 * mad ? mid : v;
    });
  }
  // Smooth: port of Prism NodeDetail smoothPing (weighted neighbours,
  // never across a null or an offline gap > 2.5x the bucket step).
  function latMinStep(points) {
    var step = Infinity;
    for (var i = 1; i < points.length; i++) {
      var gap = points[i][0] - points[i - 1][0];
      if (gap > 0) step = Math.min(step, gap);
    }
    return step;
  }
  function latSmooth(points, values, radius) {
    var step = latMinStep(points);
    if (!isFinite(step)) return values;
    return values.map(function (value, i) {
      if (value === null) return null;
      var sum = value * (radius + 1), weight = radius + 1;
      [-1, 1].forEach(function (dir) {
        var prev = i;
        for (var off = 1; off <= radius; off++) {
          var next = i + dir * off;
          if (next < 0 || next >= values.length || values[next] === null ||
              Math.abs(points[next][0] - points[prev][0]) > step * 2.5) break;
          var part = radius + 1 - off;
          sum += values[next] * part; weight += part; prev = next;
        }
      });
      return sum / weight;
    });
  }

  function latLine(text) {
    var line = document.createElement('div');
    line.style.cssText = 'font-size:11px;line-height:1.5;color:#6b7280;white-space:normal;word-break:break-word;margin-top:2px';
    line.textContent = text;
    return line;
  }

  function latChart(name, st, probes) {
    var wrap = document.createElement('div');
    // Server history is fetched on demand: expanding a node (or
    // switching range) draws the curve as soon as the one fast
    // round-trip lands, instead of waiting for samples to accrue.
    var histKey = latSrvKey(name, st.range);
    var histState = latSrvState[histKey];
    if (histState !== 'ready') {
      if (histState === 'error') {
        var err = document.createElement('div');
        err.style.cssText = 'font-size:11px;color:#6b7280;margin-top:4px';
        err.textContent = '曲线加载失败：服务端历史接口未返回。';
        wrap.appendChild(err);
        var retry = document.createElement('button');
        retry.textContent = '重试';
        retry.style.cssText = 'border:1px solid #d1d5db;background:transparent;color:#374151;border-radius:6px;padding:2px 10px;font-size:11px;cursor:pointer;margin-top:4px';
        retry.addEventListener('click', function () { delete latSrvState[histKey]; fetchLatHistory(name, st.range); inject(); });
        wrap.appendChild(retry);
        return wrap;
      }
      if (histState !== 'loading') fetchLatHistory(name, st.range);
      var loading = document.createElement('div');
      loading.style.cssText = 'font-size:11px;color:#6b7280;margin-top:4px';
      loading.textContent = '正在加载曲线…';
      wrap.appendChild(loading);
      return wrap;
    }
    var series = latSeries(name, st.range, probes);
    var hasData = series.some(function (s) { return s.points.some(function (p) { return p[1] !== null; }); });
    if (!hasData) {
      var empty = document.createElement('div');
      empty.style.cssText = 'font-size:11px;color:#6b7280;margin-top:4px';
      empty.textContent = (series.length === 0)
        ? '未设置探针：在后台探针页填入地址并保存后，服务端开始记录，展开节点即可看到曲线。'
        : '已有探针，暂无有效记录：服务端从部署起开始攒曲线，稍后回来就有曲线。';
      wrap.appendChild(empty);
      return wrap;
    }
    var radius = st.range <= 6 ? 2 : 1;
    var drawn = series.map(function (s) {
      var raw = s.points.map(function (p) { return p[1]; });
      var vals = raw;
      if (st.despike) vals = latDespike(vals);
      if (st.smooth) vals = latSmooth(s.points, vals, radius);
      return { key: s.key, points: s.points, raw: raw, vals: vals };
    });
    var ymax = 0;
    drawn.forEach(function (s) { s.vals.forEach(function (v) { if (v !== null && v > ymax) ymax = v; }); });
    ymax = Math.max(50, Math.ceil(ymax * 1.1));
    // Nice y ticks (never fractional equal divisions like
    // 0/146/291/437/582): step from 1/2/5 x 10^n, first step whose
    // interval count ceil(ymax/step) is <= 6 (5-7 levels; 582 ->
    // 100 -> 0..600, 390 -> 100 -> 0..400, 150 -> 50 -> 0..150).
    // The scale top is the nice top, so data rescales to it.
    var yStep = 10, yTop = ymax;
    (function () {
      var pow = 1;
      while (true) {
        var ms = [1, 2, 5];
        for (var mi = 0; mi < ms.length; mi++) {
          var st = ms[mi] * pow;
          var top = Math.ceil(ymax / st) * st;
          if (top / st <= 6) { yStep = st; yTop = top; return; }
        }
        pow *= 10;
      }
    })();
    var t0 = Infinity, t1 = 0;
    drawn.forEach(function (s) { s.points.forEach(function (p) { if (p[0] < t0) t0 = p[0]; if (p[0] > t1) t1 = p[0]; }); });
    if (t1 <= t0) t1 = t0 + 60;
    var W = 600, H = 180, PL = 44, PR = 8, PT = 8, PB = 20;
    var X = function (ts) { return PL + (ts - t0) / (t1 - t0) * (W - PL - PR); };
    var Y = function (v) { return PT + (1 - v / yTop) * (H - PT - PB); };
    var svgNS = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(svgNS, 'svg');
    svg.setAttribute('viewBox', '0 0 ' + W + ' ' + H);
    svg.style.cssText = 'width:100%;height:180px;display:block;background:#f9fafb;border-radius:6px;touch-action:pan-y';
    svg.setAttribute('data-ssr-chart', '');
    for (var tv = 0; tv <= yTop; tv += yStep) {
      var y = PT + (1 - tv / yTop) * (H - PT - PB);
      var ln = document.createElementNS(svgNS, 'line');
      ln.setAttribute('x1', PL); ln.setAttribute('x2', W - PR);
      ln.setAttribute('y1', y); ln.setAttribute('y2', y);
      ln.setAttribute('stroke', '#e5e7eb'); ln.setAttribute('stroke-width', '1');
      ln.setAttribute('data-ssr-grid', '');
      svg.appendChild(ln);
      var tx = document.createElementNS(svgNS, 'text');
      tx.setAttribute('x', PL - 4); tx.setAttribute('y', y + 4);
      tx.setAttribute('text-anchor', 'end'); tx.setAttribute('font-size', '10'); tx.setAttribute('fill', '#9ca3af');
      tx.textContent = tv + 'ms';
      svg.appendChild(tx);
    }
    var fmtT = function (ts) {
      var d = new Date(ts * 1000);
      var p = function (n) { return (n < 10 ? '0' : '') + n; };
      return st.range >= 168
        ? (p(d.getMonth() + 1) + '-' + p(d.getDate()) + ' ' + p(d.getHours()) + ':' + p(d.getMinutes()))
        : (p(d.getHours()) + ':' + p(d.getMinutes()));
    };
    // X ticks: whole-hour anchored (never data-boundary anchored —
    // that is why 24h used to read 23:24/03:52/08:20). Interval by
    // range: 1h -> 5min, 6h -> 30min, 24h -> 2h, 7d -> 12h; midnight
    // ticks on 24h/7d read as dates (day boundaries). If the tick
    // spacing on the plot would fall below ~36 viewBox units the
    // interval doubles until labels cannot overlap (doubling keeps
    // whole-hour anchors); a short window with fewer than two
    // anchors refines down to 10min instead of going bare.
    var pad2 = function (n) { return (n < 10 ? '0' : '') + n; };
    var hhmm = function (ts) { var d = new Date(ts * 1000); return pad2(d.getHours()) + ':' + pad2(d.getMinutes()); };
    var xTickList = function () {
      var step = st.range <= 1 ? 300 : st.range <= 6 ? 1800 : st.range <= 24 ? 7200 : 43200;
      var mk = function (stepSec) {
        var out = [];
        var d = new Date(t0 * 1000);
        if (stepSec < 3600) {
          d.setSeconds(0, 0);
          d.setMinutes(Math.ceil(d.getMinutes() / (stepSec / 60)) * (stepSec / 60));
        } else if (stepSec < 86400) {
          d.setMinutes(0, 0, 0);
          d.setHours(Math.ceil(d.getHours() / (stepSec / 3600)) * (stepSec / 3600));
        } else {
          d.setHours(0, 0, 0, 0);
        }
        // Flooring the small fields can land the anchor before t0
        // (e.g. t0 22:30 -> 22:00 for an hourly step); step forward
        // until the anchor is inside the window.
        while (d.getTime() / 1000 < t0) d = new Date(d.getTime() + stepSec * 1000);
        for (var ts = Math.round(d.getTime() / 1000); ts <= t1; ts += stepSec) out.push(ts);
        return out;
      };
      var ticks = mk(step);
      var guard = 0;
      while (ticks.length > 1 && guard++ < 8) {
        var minGap = Infinity;
        for (var i = 1; i < ticks.length; i++) minGap = Math.min(minGap, X(ticks[i]) - X(ticks[i - 1]));
        if (minGap >= 36) break;
        step *= 2;
        ticks = mk(step);
      }
      guard = 0;
      while (ticks.length < 2 && step > 600 && guard++ < 8) {
        step = Math.max(600, step / 2);
        ticks = mk(step);
      }
      if (ticks.length < 2) ticks = [t0, t1];
      return ticks;
    };
    xTickList().forEach(function (ts) {
      var tx = document.createElementNS(svgNS, 'text');
      tx.setAttribute('x', Math.max(16, Math.min(W - 16, X(ts)))); tx.setAttribute('y', H - 6);
      tx.setAttribute('text-anchor', 'middle'); tx.setAttribute('font-size', '10'); tx.setAttribute('fill', '#9ca3af');
      var d = new Date(ts * 1000);
      var isMidnight = d.getHours() === 0 && d.getMinutes() === 0;
      tx.textContent = (st.range >= 24 && isMidnight)
        ? (pad2(d.getMonth() + 1) + '-' + pad2(d.getDate()))
        : hhmm(ts);
      svg.appendChild(tx);
    });
    drawn.forEach(function (s, idx) {
      var color = latColor(s.key, idx);
      var step = latMinStep(s.points);
      var seg = [];
      var flush = function () {
        if (seg.length > 1) {
          var pl = document.createElementNS(svgNS, 'polyline');
          pl.setAttribute('points', seg.join(' '));
          pl.setAttribute('fill', 'none'); pl.setAttribute('stroke', color);
          pl.setAttribute('stroke-width', '1.5'); pl.setAttribute('stroke-linejoin', 'round');
          svg.appendChild(pl);
        } else if (seg.length === 1) {
          var c = document.createElementNS(svgNS, 'circle');
          var xy = seg[0].split(',');
          c.setAttribute('cx', xy[0]); c.setAttribute('cy', xy[1]); c.setAttribute('r', '2');
          c.setAttribute('fill', color);
          svg.appendChild(c);
        }
        seg = [];
      };
      s.points.forEach(function (p, i) {
        var v = s.vals[i];
        if (v === null) { flush(); return; }
        if (seg.length && isFinite(step) && p[0] - s.points[i - 1][0] > step * 2.5) flush();
        seg.push(X(p[0]).toFixed(1) + ',' + Y(v).toFixed(1));
      });
      flush();
    });
    // Hover vertical guide (2026-10-27): a dashed line through the
    // plot height at the column the tooltip is actually showing —
    // the live column, or the snapped column on a gap hover (never
    // the raw mouse x, so line and tip time always agree). Hidden
    // until hover; recreated with every chart render, so a range
    // switch or re-inject can never leave a stale misplaced line.
    // The card goes dark via ssr-chart-dark in dark mode (v21); the
    // #6b7280 grey stays visible on both light and dark cards, so
    // one grey works in both themes; 1px dashed, visible without shouting.
    var vline = document.createElementNS(svgNS, 'line');
    vline.setAttribute('data-ssr-vline', '1');
    vline.setAttribute('x1', PL); vline.setAttribute('x2', PL);
    vline.setAttribute('y1', PT); vline.setAttribute('y2', H - PB);
    vline.setAttribute('stroke', '#6b7280'); vline.setAttribute('stroke-width', '1');
    vline.setAttribute('stroke-dasharray', '4 3');
    vline.setAttribute('pointer-events', 'none');
    vline.style.display = 'none';
    svg.appendChild(vline);
    // Drag grab strip (2026-10-27, v18): declared here so showTip
    // can reference it; created after the hover rect below so it
    // sits on top of it. showTip keeps it centred on the guide line.
    var grab = null;
    // Floating tooltip (batch 6): the old static line under the chart
    // was easy to miss, and a handler on the overlay rect alone is
    // fragile if anything sits above it — so the tip now follows the
    // cursor (fixed position, pointer-events none so it never steals
    // the mouse) and the handler lives on the svg itself, which every
    // hover over the chart bubbles to. Tooltip values are raw bucket
    // values, never despiked/smoothed (Prism's rule). The time prefix
    // uses the first series that actually has points; an all-empty
    // series list cannot throw on points[0]. Gap snap (2026-10-27):
    // hovering a column where every series' nearest bucket is empty
    // used to show only dashes (56idc 00:30 screenshot) — the tip now
    // snaps to the nearest column with data and shows that column's
    // values with its own timestamp, so time and values never disagree.
    var tip = document.createElement('div');
    tip.style.cssText = 'position:fixed;z-index:9999;pointer-events:none;background:#111827;color:#fff;padding:4px 8px;border-radius:6px;font-size:11px;display:none';
    var showTip = function (e) {
      var rect = svg.getBoundingClientRect();
      if (!rect.width || !rect.height) return;
      // The svg is width:100% + height:180px over a 600x180 viewBox,
      // rendered with the default preserveAspectRatio meet: on a
      // desktop-width container the 600-unit content is centred with
      // empty margins, so (clientX-left)/width*W points at the wrong
      // column — the tip box still sits on the mouse, which is why
      // the tip looked right while the line landed elsewhere (the
      // 08:20 tip / 3/4 line screenshot). Map through the real
      // content rect instead; on narrow screens this reduces to
      // exactly the old formula.
      var vScale = Math.min(rect.width / W, rect.height / H);
      if (!(vScale > 0)) return;
      var vOffX = (rect.width - W * vScale) / 2;
      var px = (e.clientX - rect.left - vOffX) / vScale;
      var ts = t0 + (px - PL) / (W - PL - PR) * (t1 - t0);
      // Value lookup at a timestamp: each series contributes its
      // nearest point's raw bucket value (null = gap bucket -> dash).
      var lookup = function (at) {
        return drawn.map(function (s) {
          var best = -1, bd = Infinity;
          s.points.forEach(function (p, i) {
            var d = Math.abs(p[0] - at);
            if (d < bd) { bd = d; best = i; }
          });
          return best >= 0 ? s.raw[best] : null;
        });
      };
      var vals = lookup(ts);
      var tSeries = null;
      for (var si = 0; si < drawn.length; si++) {
        if (drawn[si].points.length) { tSeries = drawn[si]; break; }
      }
      var showTs = tSeries ? tSeries.points.reduce(function (acc, p) {
        return Math.abs(p[0] - ts) < Math.abs(acc - ts) ? p[0] : acc;
      }, tSeries.points[0][0]) : null;
      // Gap snap: when the hovered column is a gap bucket in every
      // series, a plain lookup shows only dashes (the 56idc 00:30
      // screenshot). Snap to the nearest column that actually has
      // data (ties -> earlier column) and show that column's values
      // with its own timestamp, so time and values never disagree.
      // Only when no column in the whole chart has data do we say so.
      var snapTs = null;
      var allDash = vals.every(function (v) { return v === null || v === undefined; });
      if (allDash) {
        var snapD = Infinity;
        drawn.forEach(function (s) {
          s.points.forEach(function (p, i) {
            if (s.raw[i] === null || s.raw[i] === undefined) return;
            var d = Math.abs(p[0] - ts);
            if (d < snapD) { snapD = d; snapTs = p[0]; }
          });
        });
        if (snapTs !== null) {
          vals = lookup(snapTs);
          showTs = snapTs;
        }
      }
      // Guide line at the column the tip shows (snapped column on
      // gaps), never the raw mouse x; hidden when the tip shows no
      // column at all (the defensive all-empty case). The drag grab
      // strip rides centred on the line (~20 units wide, transparent)
      // so a finger can actually catch the 1px line.
      if (showTs !== null && !(allDash && snapTs === null)) {
        var vx = X(showTs);
        vline.setAttribute('x1', vx); vline.setAttribute('x2', vx);
        vline.style.display = 'block';
        if (grab) {
          grab.setAttribute('x', Math.max(PL, Math.min(vx - 10, W - PR - 20)));
          grab.setAttribute('width', 20);
          grab.style.display = 'block';
        }
      } else {
        vline.style.display = 'none';
        if (grab) grab.style.display = 'none';
      }
      // Vertical layout (2026-10-27): time on its own line, one
      // probe road per line (only configured roads — drawn is built
      // from configured probes only; a missing value still shows a
      // dash). The old single line ran past the viewport's right
      // edge when hovering near it (phone screenshot: cut off after
      // "联通 v6 331"). Gap snap above is untouched: vals/showTs are
      // already the snapped column's by the time we render.
      while (tip.firstChild) tip.removeChild(tip.firstChild);
      if (allDash && snapTs === null) {
        tip.appendChild(document.createTextNode('暂无数据'));
      } else {
        if (showTs !== null) {
          var tline = document.createElement('div');
          tline.textContent = fmtT(showTs);
          tip.appendChild(tline);
        }
        drawn.forEach(function (s, idx) {
          var v = vals[idx];
          var vline = document.createElement('div');
          vline.textContent = s.key + ' ' + (v === null || v === undefined ? '—' : v + 'ms');
          tip.appendChild(vline);
        });
      }
      tip.style.display = 'block';
      // Clamp with the tip's real size (it is now taller and narrower
      // than the old fixed 220x40 guess), so it never runs off the
      // right or bottom edge; 8px margin on every side.
      var tw = tip.offsetWidth, th = tip.offsetHeight;
      tip.style.left = Math.max(8, Math.min(e.clientX + 12, window.innerWidth - tw - 8)) + 'px';
      tip.style.top = Math.max(8, Math.min(e.clientY + 12, window.innerHeight - th - 8)) + 'px';
    };
    var hideTip = function () { tip.style.display = 'none'; vline.style.display = 'none'; if (grab) grab.style.display = 'none'; };
    var hover = document.createElementNS(svgNS, 'rect');
    hover.setAttribute('x', PL); hover.setAttribute('y', 0);
    hover.setAttribute('width', W - PL - PR); hover.setAttribute('height', H);
    hover.setAttribute('fill', 'transparent');
    hover.setAttribute('style', 'touch-action:pan-y');
    svg.appendChild(hover);
    // Grab strip on top of the hover rect: transparent but
    // hit-testable, full plot height, centred on the guide line by
    // showTip. Pointerdown anywhere on the chart starts a scrub drag
    // (see listeners below); the strip exists so the line itself is
    // catchable by a finger, not just the plot in general.
    grab = document.createElementNS(svgNS, 'rect');
    grab.setAttribute('data-ssr-vgrab', '1');
    grab.setAttribute('y', 0); grab.setAttribute('height', H);
    grab.setAttribute('fill', 'transparent');
    grab.setAttribute('pointer-events', 'all');
    grab.setAttribute('style', 'touch-action:pan-y');
    grab.style.display = 'none';
    svg.appendChild(grab);
    // Draggable guide (2026-10-27, v18): pointer events unify mouse
    // and touch. Press-and-drag anywhere on the chart scrubs the
    // guide; the tip follows with exactly the hover lookup + gap
    // snap, so dragged values agree with hovered values. On release
    // the guide + tip STAY at the last column (pinned) for reading,
    // until the next hover move on the chart (position changed) or a
    // re-render (new chart elements, fresh state) clears them.
    // Pointer capture keeps the drag tracking past the chart edge;
    // touch-action pan-y on the rects lets horizontal finger drags
    // reach us while vertical page scroll still works. Desktop
    // hover is unchanged: plain mousemove still shows/follows.
    var drag = { active: false, pinned: false, id: null, x: 0, y: 0 };
    var onHoverMove = function (e) {
      if (drag.active) return; // pointermove drives the drag
      if (drag.pinned) {
        // Same-point compatibility mouse event after a touch release
        // (or a 1px jitter): not a new hover, keep the pin.
        if (Math.abs(e.clientX - drag.x) < 3 && Math.abs(e.clientY - drag.y) < 3) return;
        drag.pinned = false;
      }
      showTip(e);
    };
    var onHoverLeave = function () {
      if (drag.active || drag.pinned) return;
      hideTip();
    };
    var onDragStart = function (e) {
      drag.active = true; drag.pinned = false;
      drag.id = (e.pointerId !== undefined ? e.pointerId : null);
      if (typeof svg.setPointerCapture === 'function') {
        try { svg.setPointerCapture(e.pointerId); } catch (err) {}
      }
      showTip(e);
    };
    var onDragMove = function (e) {
      if (!drag.active) return;
      if (drag.id !== null && e.pointerId !== undefined && e.pointerId !== drag.id) return;
      showTip(e);
    };
    var onDragEnd = function (e) {
      if (!drag.active) return;
      if (drag.id !== null && e.pointerId !== undefined && e.pointerId !== drag.id) return;
      drag.active = false; drag.pinned = true; drag.x = e.clientX; drag.y = e.clientY;
      // Guide + tip deliberately left where the drag ended.
    };
    svg.addEventListener('mousemove', onHoverMove);
    svg.addEventListener('mouseleave', onHoverLeave);
    svg.addEventListener('pointerdown', onDragStart);
    svg.addEventListener('pointermove', onDragMove);
    svg.addEventListener('pointerup', onDragEnd);
    svg.addEventListener('pointercancel', onDragEnd);
    var legend = document.createElement('div');
    legend.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px;font-size:11px;color:#374151;margin-top:4px';
    drawn.forEach(function (s, idx) {
      var last = null;
      for (var i = s.raw.length - 1; i >= 0; i--) { if (s.raw[i] !== null) { last = s.raw[i]; break; } }
      var item = document.createElement('span');
      var dot = document.createElement('span');
      dot.style.cssText = 'display:inline-block;width:8px;height:8px;border-radius:50%;background:' + latColor(s.key, idx) + ';margin-right:3px';
      item.appendChild(dot);
      item.appendChild(document.createTextNode(s.key + (last !== null ? ' ' + last + 'ms' : ' —')));
      legend.appendChild(item);
    });
    wrap.appendChild(legend);
    wrap.appendChild(svg);
    wrap.appendChild(tip);
    return wrap;
  }

  function latControls(name, st) {
    var bar = document.createElement('div');
    bar.style.cssText = 'display:flex;flex-wrap:wrap;gap:6px;align-items:center;margin-top:4px';
    var mkBtn = function (label, active, fn) {
      var b = document.createElement('button');
      b.textContent = label;
      b.style.cssText = 'border:1px solid ' + (active ? '#f97316' : '#d1d5db') + ';background:' + (active ? '#f97316' : 'transparent') + ';color:' + (active ? '#fff' : '#374151') + ';border-radius:6px;padding:2px 8px;font-size:11px;cursor:pointer';
      b.addEventListener('click', fn);
      return b;
    };
    [[1, '1h'], [6, '6h'], [24, '24h'], [168, '7d']].forEach(function (r) {
      bar.appendChild(mkBtn(r[1], st.range === r[0], function () { st.range = r[0]; inject(); }));
    });
    bar.appendChild(mkBtn('削峰', st.despike, function () { st.despike = !st.despike; inject(); }));
    bar.appendChild(mkBtn('平滑', st.smooth, function () { st.smooth = !st.smooth; inject(); }));
    return bar;
  }

  function summaryLine(s) {
    var parts = [s.display];
    parts.push((s.online4 || s.online6) ? '在线' : '离线');
    if (s.location) parts.push(s.location);
    parts.push('负载 ' + s.load_1 + ' / ' + s.load_5 + ' / ' + s.load_15);
    if (typeof s.cpu === 'number') parts.push('CPU ' + (Math.round(s.cpu * 10) / 10) + '%');
    if (s.memory_total > 0) parts.push('内存 ' + (Math.round(s.memory_used / s.memory_total * 1000) / 10) + '%');
    return parts.join(' · ');
  }

  // Tabbed detail block (batch 5): summary row on top, then 网络延迟 /
  // 系统概览 tabs. The latency tab carries the live probe lines, the
  // range + 削峰/平滑 controls and the server-history chart; the overview tab
  // carries detailLines (plan incl. end date, system, CPU, traffic,
  // boot, online, activity, version, public note) — probe lines live
  // only on the latency tab, so they are filtered out here defensively.
  function renderDetailBlock(srv, block) {
    var st = detailStateFor(srv.name);
    var sum = document.createElement('div');
    sum.style.cssText = 'font-size:12px;font-weight:600;color:#111827;margin-top:2px';
    sum.textContent = summaryLine(srv);
    block.appendChild(sum);
    var tabs = document.createElement('div');
    tabs.style.cssText = 'display:flex;gap:6px;margin-top:6px';
    [['latency', '网络延迟'], ['overview', '系统概览']].forEach(function (t) {
      var b = document.createElement('button');
      b.textContent = t[1];
      var active = st.tab === t[0];
      b.style.cssText = 'border:1px solid ' + (active ? '#0a6cff' : '#d1d5db') + ';background:' + (active ? '#0a6cff' : 'transparent') + ';color:' + (active ? '#fff' : '#374151') + ';border-radius:6px;padding:2px 10px;font-size:11px;cursor:pointer';
      b.addEventListener('click', function () { st.tab = t[0]; inject(); });
      tabs.appendChild(b);
    });
    block.appendChild(tabs);
    if (st.tab === 'latency') {
      probeLines(srv).forEach(function (text) { block.appendChild(latLine(text)); });
      block.appendChild(latControls(srv.name, st));
      block.appendChild(latChart(srv.name, st, srv.ping_probes));
    } else {
      var probes = probeLines(srv);
      detailLines(srv).forEach(function (text) {
        if (probes.indexOf(text) >= 0) return; // probe lines live on the latency tab
        block.appendChild(latLine(text));
      });
    }
  }

  // Batch 3: swap the main panel's 节点 (name) and 协议 (IPv4/IPv6
  // badges) columns — protocol first, node second, everything else in
  // place, contents untouched. The React bundle is precompiled, so the
  // swap is a DOM reorder re-applied on every inject (React wipes
  // foreign DOM on refresh). Idempotent: a row whose first cell is
  // already the protocol cell (no alias div.truncate inside) is left
  // alone, and the header is swapped only while it still reads 节点/协议.
  function swapColumns() {
    var table = document.getElementById('ssr-table');
    if (!table) return;
    var head = table.querySelector('thead tr');
    if (head) {
      var ths = head.querySelectorAll('th');
      if (ths.length >= 2 && (ths[0].textContent || '').trim() === '节点'
          && (ths[1].textContent || '').trim() === '协议') {
        head.insertBefore(ths[1], ths[0]);
      }
    }
    var rows = table.querySelectorAll('tbody tr');
    rows.forEach(function (tr) {
      if (tr.getAttribute && tr.getAttribute('data-ssr-detail-row')) return;
      var tds = tr.querySelectorAll('td');
      if (tds.length < 2) return; // React's own expand row: single colspan td
      var first = tds[0];
      if (first.querySelector && first.querySelector('div.truncate')) {
        tr.insertBefore(tds[1], first);
      }
    });
  }

  // Hide React's own expanded detail block (the centered load/memory/
  // swap/disk/TCP/UDP/latency block plus its #0/icon tail): the user
  // asked to delete that part and keep only the overlay's tabbed
  // block. The native nodes live in the same single colspan td as our
  // block, so hide every direct child of the td except
  // [data-ssr-detail] — no reliance on the native block's internal
  // structure, so the #0/icon tail is hidden too. This runs on EVERY
  // inject pass, open or collapsed: React recreates the native nodes
  // on refresh, so a once-only hide would let them resurrect.
  // Style-only changes do not trip the MutationObserver loop, and
  // inject() changes no classes, so the class-watching observer
  // cannot loop on itself either. The expand row itself is never
  // touched, so expand/collapse keeps working.
  function hideNativeDetail(reactTd) {
    if (!reactTd) return;
    Array.prototype.slice.call(reactTd.children).forEach(function (el) {
      if (el.getAttribute && el.getAttribute('data-ssr-detail') != null) return;
      if (el.style && el.style.display !== 'none') el.style.display = 'none';
    });
  }

  function inject() {
    var names = Object.keys(planByName);
    if (!names.length) return;
    // Batch 4: v7's separate detail rows are gone (relocated into
    // React's own detail block, see below); drop any still in the
    // DOM from the old script so they never stack under the new block.
    Array.prototype.slice.call(document.querySelectorAll('tr[data-ssr-detail-row]')).forEach(function (r) {
      if (r.parentNode) r.parentNode.removeChild(r);
    });
    renderPills();
    swapColumns();
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
      // React's own expand row, computed once per row (reused by the
      // indicator and the detail attach below). React toggles
      // expansion on a WHOLE-ROW click (bundle: tr onClick) and the
      // expand row stays in the DOM, flipping only its class
      // (visible/collapse) plus the inner div's max-h — a toggle
      // produces NO childList mutation, so the old childList-only
      // observer never saw it and the overlay only learned about
      // non-name clicks on the next 30 s refresh (meanwhile the
      // native block showed unhidden and the overlay tabs were
      // missing — exactly the "only name clicks expand" feeling).
      // The observer now also watches class attributes, and the
      // overlay map is synced from React's class on every pass
      // below, so the map, the indicator and the live-tail list
      // always agree with React no matter which cell was clicked.
      var tr0 = td.closest('tr');
      var expandRow = (tr0 && tr0.nextElementSibling) ? tr0.nextElementSibling : null;
      var reactTd = null;
      var reactOpen = false;
      if (expandRow) {
        var rtds0 = expandRow.querySelectorAll('td');
        if (rtds0.length === 1) {
          reactTd = rtds0[0];
          reactOpen = /(^|\s)visible(\s|$)/.test(expandRow.className || '');
        }
      }
      // Single source of truth (v21): React's visible/collapse
      // class. The map used to toggle only on name-cell clicks, so
      // it drifted whenever React toggled via any other cell — and
      // the next refresh then re-attached the block into the
      // collapsed row and left the indicator wrong. Sync it here so
      // the map, the indicator and the live-tail list always match
      // React. Only sync when the expand row has the expected
      // single-td shape; otherwise leave the map alone.
      if (reactTd) {
        if (reactOpen) expanded[srv.name] = true;
        else delete expanded[srv.name];
      }
      // Hide React's native detail block on every pass (see
      // hideNativeDetail): pre-hiding while collapsed means no flash
      // of the native block between a click and the next inject, and
      // a React re-render can never resurrect it.
      hideNativeDetail(reactTd);
      var open = reactOpen || !!expanded[srv.name];
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
      var wantInd = open ? '▾' : '▸';
      if (ind.textContent !== wantInd) ind.textContent = wantInd;
      // Tag filter: hide rows outside the active pill's Tag. Unmapped
      // rows (name not found in stats) always stay visible.
      var tr = td.closest('tr');
      var wantDisp = '';
      if (tr) {
        wantDisp = (activeTag !== null && (srv.tag || '') !== activeTag) ? 'none' : '';
        if (tr.style.display !== wantDisp) tr.style.display = wantDisp;
      }
      // Main-panel location column: replace the place-name text with a
      // flag image (batch 3). The React cell is td.w-11, the only cell
      // of that width in the row. Only touch it when its text is this
      // node's location and stats carries a country code — never guess
      // a flag, and never touch a cell that does not match (bundle
      // changes must fail safe, not clobber another column). React
      // wipes foreign DOM on refresh, so re-apply when the marker is
      // gone; the marker makes a second pass a no-op. This runs for
      // every row, not just expanded ones (it sits before the early
      // return below). The title keeps the place name on hover.
      if (tr && srv.location && srv.country_code) {
        var locTd = tr.querySelector('td.w-11');
        if (locTd) {
          var locText = (locTd.textContent || '').trim();
          if ((locText === srv.location || locText.endsWith(srv.location))
              && !locTd.querySelector('[data-ssr-flag]')) {
            locTd.innerHTML = '';
            var flag = document.createElement('img');
            flag.setAttribute('data-ssr-flag', '1');
            flag.src = 'https://flagcdn.com/w40/' + srv.country_code.toLowerCase() + '.png';
            flag.alt = srv.country_code;
            flag.title = srv.location;
            flag.style.cssText = 'width:20px;vertical-align:middle';
            flag.onerror = function () { this.style.display = 'none'; };
            locTd.appendChild(flag);
          }
        }
      }
      // Detail block (batch 5, Prism port): the tabbed block lives
      // INSIDE React's own expanded row — appended at the end of its
      // single colspan td, while React's own centered detail block
      // in the same td is hidden by hideNativeDetail above, so only
      // the overlay block shows. No table row or cell of
      // our own: batch 3's separate full-width row is gone, and
      // nothing is ever crammed into the narrow name cell again.
      // React wipes foreign DOM on refresh, so the block is
      // re-attached on every inject (same pattern as the pills). If
      // React's row is not in the DOM yet (it renders just after the
      // click), nothing is rendered anywhere; the next
      // MutationObserver pass attaches it. Block content is plain
      // divs/buttons/svg (no div.truncate class) so the name-cell
      // loop above never mistakes them for node rows. Present iff
      // expanded, rebuilt only when its key changes (stats lines +
      // tab/range/toggle state + latest sample); tab state lives in
      // detailState (overlay JS), not in the DOM, so a React wipe +
      // re-attach keeps the tab the user picked. A legacy in-cell
      // block from an older overlay version is removed so the two
      // never stack.
      var legacy = td.querySelector('[data-ssr-detail]');
      if (legacy) legacy.remove();
      // reactTd/reactOpen/open were computed at the top of this row's
      // pass (React's expand row: next sibling, single colspan td;
      // any other shape is rejected, never attached into). The gate
      // is `open`, which now always equals React's class (the map
      // is synced from it above), so a whole-row click on any cell
      // ends up rendering — the old map-only gate rendered nothing
      // for non-name clicks, exactly the "tabs never appear" shape.
      if (!open) {
        if (reactTd) {
          var stale = reactTd.querySelector('[data-ssr-detail]');
          if (stale) stale.remove();
        }
        return;
      }
      if (!reactTd) return; // No single-td expand row; retry next inject.
      var st = detailStateFor(srv.name);
      var key = detailLines(srv).join('\n')
        + '|tab=' + st.tab + '|range=' + st.range
        + '|d=' + (st.despike ? 1 : 0) + '|s=' + (st.smooth ? 1 : 0)
        + '|hist=' + (latSrvState[latSrvKey(srv.name, st.range)] || 'none')
        + '|live=' + (latLiveTs[srv.name] || 0);
      var block = reactTd.querySelector('[data-ssr-detail]');
      if (block) {
        if (block.getAttribute('data-ssr-key') === key) return;
        block.parentNode.removeChild(block);
        block = null;
      }
      block = document.createElement('div');
      block.setAttribute('data-ssr-detail', '1');
      block.setAttribute('data-ssr-key', key);
      block.style.cssText = 'margin-top:10px;padding-top:8px;border-top:1px solid #e5e7eb;text-align:left';
      renderDetailBlock(srv, block);
      reactTd.appendChild(block);
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
    // Deliberately no stopPropagation/preventDefault and no map
    // toggle: React owns the toggle (whole-row onClick) and the map
    // is synced from React's visible/collapse class in inject, so
    // toggling here as well would double-toggle the map out of
    // sync. This handler only forces an immediate inject pass; the
    // class-watching observer covers the flip itself, so a click on
    // ANY cell — not just the name cell — ends up synced.
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
            location: s.location || '',
            country_code: s.country_code || '',
            plan_start_date: s.plan_start_date || '',
            plan_end_date: s.plan_end_date || '',
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
            // uptime_sec = raw seconds (boot time); uptime_display =
            // the formatted string ("N 天" / "HH:MM:SS", the stats
            // "uptime" key). Never feed the string into bootLine.
            uptime_sec: s.uptime_sec || 0,
            uptime_display: s.uptime || '',
            public_note: s.public_note || '',
            sys_info: s.sys_info || null,
            ping_probes: s.ping_probes || [],
          };
        });
        planByName = map;
        haveStats = true;
        // Server history now feeds the chart; each refresh only
        // appends a live tail point for expanded nodes, so the line
        // keeps growing between history fetches.
        var nowSec = Math.floor(Date.now() / 1000);
        Object.keys(expanded).forEach(function (n) {
          if (planByName[n]) appendLatLive(planByName[n], nowSec);
        });
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
    ensureHeaderFixStyle();
    ensureDetailDarkStyle();
    ensureChartDarkStyle();
    refresh();
    setInterval(refresh, 30000);
    document.addEventListener('click', onDocClick);
    new MutationObserver(scheduleInject).observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] });
    setTimeout(inject, 1500);
    setTimeout(inject, 5000);
  });
})();
