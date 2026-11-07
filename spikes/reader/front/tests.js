const sleep = ms => new Promise(r => setTimeout(r, ms))
const frames = view => view.renderer.getContents?.() ?? []
export async function runTests(view, cfg, rep, inv) {
  if (cfg.test === 'events') { await eventTests(view, rep, inv); await rep('DONE'); return }
  if (cfg.test === 'perf') { await perfTest(view, cfg, rep); await rep('DONE'); return }
  if (cfg.test === 'stress') { await stressTest(view, cfg, rep, inv); await rep('DONE'); return }
  if (cfg.test === 'diag') {
    const seen = new Set()
    view.addEventListener('load', e => e.detail.doc.addEventListener('securitypolicyviolation', v => { const k = v.effectiveDirective + ' ' + v.blockedURI.slice(0, 80); if (!seen.has(k)) { seen.add(k); rep('CSPV ' + k) } }))
    for (const i of cfg.secs) {
      const t0 = performance.now()
      const ok = await withTimeout(view.goTo(i), 15000).then(() => true, e => String(e))
      await sleep(600)
      const s = view.book.sections[i], c = (view.renderer.getContents?.() ?? []).find(x => x.index === i)
      const d = c?.doc
      await rep(`DIAG sec=${i} id=${s.id} linear=${s.linear} goTo=${ok} ms=${Math.round(performance.now() - t0)} size=${await s.size} text=${d?.body?.textContent?.trim().length} kids=${d?.body?.children?.length} html=${JSON.stringify(d?.body?.innerHTML?.slice(0, 160))}`)
      if (d?.fonts) await rep('FONTS ' + JSON.stringify([...d.fonts].filter(f => f.status === 'error').map(f => f.family).slice(0, 6)))
      await inv('spike_snapshot', { name: cfg.id + '-s' + i })
    }
    await rep('DONE'); return
  }
  if (cfg.test === 'nav') { await view.goTo(0); await sleep(3000); await rep('after nav top=' + location.href); await rep('DONE'); return }
  if (cfg.test === 'hostile') {
    await view.renderer.setStyles?.('')
    const n = view.book.sections.length
    for (let i = 0; i < n; i++) {
      if (view.book.sections[i].linear === 'no') continue
      await view.goTo(i).catch(e => rep('goTo err ' + i + ' ' + e))
      await sleep(1800)
      for (const { doc } of frames(view)) {
        const imgs = [...doc.querySelectorAll('img[data-probe]')]
        for (const im of imgs) rep(`IMG ${im.dataset.probe} loaded=${im.naturalWidth > 0}`)
        rep(`section ${i} rendered url=${doc.URL.slice(0, 40)} sandbox=${doc.defaultView.frameElement?.getAttribute('sandbox')} text=${JSON.stringify(doc.body.textContent.trim().slice(0, 40))}`)
      }
    }
  }
}

const withTimeout = (p, ms) => Promise.race([p, new Promise((_, r) => setTimeout(() => r(new Error('timeout')), ms))])
const waitLoad = (view, i, ms) => new Promise(res => {
  const t = setTimeout(() => { view.removeEventListener('load', f); res(false) }, ms)
  const f = e => { if (e.detail.index === i) { clearTimeout(t); view.removeEventListener('load', f); res(true) } }
  view.addEventListener('load', f)
})
export async function surveyBook(view, id, rep) {
  const r = { id, sections: 0, walked: 0, blank: [], timeouts: [], brokenImgs: 0, imgs: 0, fonts: {}, cspv: {}, fxl: view.book.rendition?.layout === 'pre-paginated', turnMs: [], cfiOk: 0, cfiBad: 0, encrypted: false }
  const onV = v => { r.cspv[v.effectiveDirective] = (r.cspv[v.effectiveDirective] ?? 0) + 1 }
  view.addEventListener('load', e => e.detail.doc.addEventListener('securitypolicyviolation', onV))
  const secs = view.book.sections
  r.sections = secs.length
  for (let i = 0; i < secs.length; i++) {
    if (secs[i].linear === 'no') continue
    const t0 = performance.now()
    try { await withTimeout(view.goTo(i), 8000) } catch { r.timeouts.push(i); continue }
    await sleep(40)
    const c = (view.renderer.getContents?.() ?? []).find(x => x.index === i)
    if (!c?.doc?.body) { r.blank.push([i, 'nodoc']); continue }
    r.walked++
    const { doc } = c
    const expected = (await secs[i].size) ?? 0
    const text = doc.body.textContent.trim().length
    const media = doc.querySelectorAll('img,svg,image,video').length
    const h = doc.documentElement.getBoundingClientRect().width + doc.documentElement.getBoundingClientRect().height
    if (!text && !media && expected > 1500) r.blank.push([i, 'empty', expected])
    if (h === 0) r.blank.push([i, 'zerosize'])
    for (const im of doc.images) { r.imgs++; if (im.complete && im.naturalWidth === 0) r.brokenImgs++ }
    if (doc.fonts) { try { await withTimeout(doc.fonts.ready, 1500) } catch {} ; for (const f of doc.fonts) r.fonts[f.status] = (r.fonts[f.status] ?? 0) + 1 }
    r.lastMs = Math.round(performance.now() - t0)
    r.maxMs = Math.max(r.maxMs ?? 0, r.lastMs)
  }
  // page turns and CFI round trip in a middle section
  try {
    const mid = secs.findIndex((s, i) => i >= secs.length / 2 && s.linear !== 'no')
    await withTimeout(view.goTo(Math.max(mid, 0)), 8000)
    for (let k = 0; k < 6; k++) {
      const t0 = performance.now(); await withTimeout(view.next(), 6000); r.turnMs.push(Math.round(performance.now() - t0))
      await sleep(30)
      const loc = view.lastLocation
      if (k % 2 === 1 && loc?.cfi) {
        const before = { cfi: loc.cfi, frac: loc.fraction, idx: loc.section?.current }
        await view.prev(); await sleep(30)
        await withTimeout(view.goTo(before.cfi), 6000); await sleep(60)
        const after = view.lastLocation
        if (after?.section?.current === before.idx && Math.abs((after?.fraction ?? 0) - before.frac) < 0.01) r.cfiOk++
        else { r.cfiBad++; r.cfiDetail = [before.frac, after?.fraction] }
      }
    }
  } catch (e) { r.turnErr = String(e).slice(0, 100) }
  await rep('BOOK ' + JSON.stringify(r))
}

import { FootnoteHandler } from './foliate/footnotes.js'
export async function eventTests(view, rep, inv) {
  const c = { click: 0, mousedown: 0, keydown: 0, wheel: 0, selectionchange: 0 }
  const seen = { link: 0, external: 0, relocate: 0, draw: 0, show: 0, footnote: '' }
  await view.goTo(0); await sleep(800)
  const content = () => view.renderer.getContents()[0]
  const hook = doc => { for (const t of Object.keys(c)) doc.addEventListener(t, () => c[t]++)
    doc.addEventListener('click', e => rep(`  clickdbg target=${e.target.nodeName}#${e.target.id} xy=${e.clientX},${e.clientY} detail=${e.detail}`))
    doc.addEventListener('mousemove', e => { c.mm = (c.mm ?? 0) + 1; c.btn = e.buttons })
    doc.addEventListener('mouseup', e => rep(`  mouseup xy=${e.clientX},${e.clientY} mm=${c.mm} btn=${c.btn} sel=${JSON.stringify(doc.getSelection().toString().slice(0,30))}`))
    doc.addEventListener('dragstart', e => rep('  dragstart ' + e.target.nodeName))
    doc.addEventListener('selectstart', e => rep('  selectstart prevented=' + e.defaultPrevented))
  }
  window.addEventListener('wheel', e => rep('  parent wheel ' + e.deltaY), { passive: true })
  hook(content().doc)
  view.addEventListener('load', e => hook(e.detail.doc))
  view.addEventListener('relocate', () => seen.relocate++)
  view.addEventListener('external-link', e => { seen.external++; e.preventDefault() })
  view.addEventListener('draw-annotation', e => { seen.draw++; e.detail.draw((rects, opts) => { const g = document.createElementNS('http://www.w3.org/2000/svg', 'g'); return g }, {}) })
  view.addEventListener('show-annotation', () => seen.show++)
  const fh = new FootnoteHandler()
  fh.addEventListener('before-render', e => { const v = e.detail.view; v.style.cssText = 'position:fixed;left:0;bottom:0;width:400px;height:120px'; document.body.append(v) })
  fh.addEventListener('render', e => { const v = e.detail.view; setTimeout(() => { seen.footnote = v.renderer?.getContents?.()[0]?.doc?.body?.textContent?.trim().slice(0, 40) ?? '' }, 400) })
  view.addEventListener('link', e => { seen.link++; fh.handle(view.book, e)?.catch(err => rep('footnote err ' + err)) })
  const at = (doc, sel) => {
    const el = doc.querySelector(sel), f = doc.defaultView.frameElement.getBoundingClientRect(), r = el.getClientRects()[0]
    return { x: f.left + r.left + Math.min(r.width / 2, 40), y: f.top + r.top + r.height / 2, r, f }
  }
  const input = (kind, x = 0, y = 0, x2 = 0, y2 = 0, key = 0) => inv('spike_input', { kind, x, y, x2, y2, key })
  // theme styles and book css under CSP
  await view.renderer.setStyles?.('html{--t:1} p{color:rgb(1,2,3) !important}')
  await sleep(200)
  const doc = content().doc
  await rep('STYLE engine=' + doc.defaultView.getComputedStyle(doc.querySelector('p')).color + ' book-css=' + doc.defaultView.getComputedStyle(doc.querySelector('h1')).color)
  // T1 click
  let p = at(doc, '#sel'); await input('click', p.x, p.y); await sleep(300)
  await rep(`T1 click frame click=${c.click} mousedown=${c.mousedown}`)
  // T2 double click
  await input('dbl', p.x, p.y); await sleep(300)
  await rep('T2 dblclick selection=' + JSON.stringify(doc.getSelection().toString()))
  // T3 drag
  doc.getSelection().removeAllRanges()
  const sc = c.selectionchange
  await input('drag', p.r.left + p.f.left + 4, p.y, p.r.left + p.f.left + 260, p.y); await sleep(400)
  const sel = doc.getSelection(), text = sel.toString()
  await rep(`T3 drag selection=${JSON.stringify(text)} selectionchange=${c.selectionchange - sc}`)
  if (text) {
    const range = sel.getRangeAt(0), idx = content().index, cfi = view.getCFI(idx, range)
    const r = await view.addAnnotation({ value: cfi })
    await sleep(200)
    const back = await view.resolveCFI(cfi)
    const rr = back.anchor(doc)
    await rep(`T3 cfi=${cfi} roundtrip=${JSON.stringify(rr?.toString?.())} equal=${rr?.toString?.() === text} draw=${seen.draw}`)
    doc.getSelection().removeAllRanges()
    await input('click', p.r.left + p.f.left + 30, p.y); await sleep(300)
    await rep(`T3 click-on-highlight show-annotation=${seen.show}`)
  }
  // T4 keyboard
  const beforeKey = c.keydown, before = view.lastLocation?.fraction
  await inv('spike_input', { kind: 'key', x: 0, y: 0, x2: 0, y2: 0, key: 124 }); await sleep(300)
  await rep(`T4 key frame-keydown=${c.keydown - beforeKey}`)
  await view.next(); await sleep(300)
  await rep(`T4 next fraction ${before} -> ${view.lastLocation?.fraction}`)
  await view.prev(); await sleep(300)
  // T5 wheel
  const wb = c.wheel, fb = view.lastLocation?.fraction
  await input('wheel', p.x, p.y, 0, -120); await sleep(100); await input('wheel', p.x, p.y, 0, -120); await sleep(500)
  await rep(`T5 wheel frame-wheel=${c.wheel - wb} fraction ${fb} -> ${view.lastLocation?.fraction}`)
  await view.goTo(0); await sleep(500)
  const d0 = content().doc
  // T6 external
  await rep(`  ext rect ${JSON.stringify(d0.querySelector('#ext').getClientRects()[0])} frame ${JSON.stringify(d0.defaultView.frameElement.getBoundingClientRect())} scroll ${view.renderer.start}`)
  let q = at(d0, '#ext'); await rep(`  ext at ${q.x},${q.y} hit=${d0.elementFromPoint(q.x - q.f.left, q.y - q.f.top)?.nodeName}`); await input('click', q.x, q.y); await sleep(500)
  await rep(`T6 external-link events=${seen.external} top=${location.href}`)
  q = at(d0, '#extb'); await input('click', q.x, q.y); await sleep(500)
  await rep(`T6 blank external events=${seen.external} top=${location.href}`)
  // T7 footnote
  q = at(d0, '#nref'); await input('click', q.x, q.y); await sleep(1500)
  await rep(`T7 noteref link=${seen.link} footnote=${JSON.stringify(seen.footnote)}`)
  // T8 internal
  const l0 = seen.link; q = at(view.renderer.getContents()[0].doc, '#lnk'); await input('click', q.x, q.y); await sleep(1000)
  await rep(`T8 internal link=${seen.link - l0} section=${view.lastLocation?.section?.current} tocLabel=${view.lastLocation?.tocItem?.label}`)
}

export async function perfTest(view, cfg, rep) {
  const secs = view.book.sections, n = secs.length
  await view.goTo(0)
  const mid = Math.floor(n / 2), times = []
  let t0 = performance.now(); await view.goTo(mid); const jump = Math.round(performance.now() - t0)
  for (let k = 0; k < 20; k++) { t0 = performance.now(); await view.next(); times.push(Math.round(performance.now() - t0)); await sleep(10) }
  times.sort((a, b) => a - b)
  await rep(`PERF id=${cfg.id} mode=${cfg.mode} jump=${jump} turn p50=${times[10]} max=${times[19]}`)
}

import * as CFI from './foliate/epubcfi.js'
const koboFilter = n => {
  if (n.nodeType !== 1) return NodeFilter.FILTER_ACCEPT
  if (n.classList?.contains('koboSpan') || n.id === 'book-columns' || n.id === 'book-inner') return NodeFilter.FILTER_SKIP
  return NodeFilter.FILTER_ACCEPT
}
const passages = doc => {
  const w = doc.createTreeWalker(doc.body, NodeFilter.SHOW_TEXT), out = []
  for (let n; (n = w.nextNode());) if (n.data.trim().length >= 40 && !n.parentElement.closest('script,style')) out.push(n)
  return out
}
const norm = s => (s ?? '').replace(/\s+/g, ' ').trim()
export async function locatorBook(plain, kepub, id, rep) {
  const r = { id, tested: 0, filtered: 0, unfiltered: 0, filteredThrow: 0, unfilteredThrow: 0, sections: 0, missingSection: 0 }
  const byHref = new Map(kepub.sections.map(s => [s.id, s]))
  const secs = plain.sections.filter(s => s.linear !== 'no')
  const step = Math.max(1, Math.floor(secs.length / 12))
  for (let i = 0; i < secs.length; i += step) {
    const s = secs[i], k = byHref.get(s.id)
    if (!k) { r.missingSection++; continue }
    r.sections++
    const pd = await s.createDocument(), kd = await k.createDocument()
    const nodes = passages(pd)
    const pick = nodes.filter((_, j) => j % Math.max(1, Math.floor(nodes.length / 6)) === 0).slice(0, 6)
    for (const n of pick) {
      const range = pd.createRange(); range.setStart(n, 5); range.setEnd(n, 35)
      const quote = range.toString()
      const parts = CFI.parse(CFI.fromRange(range))
      r.tested++
      const tryIt = (filter, key) => {
        try {
          const got = CFI.toRange(kd, parts, filter)
          if (norm(got?.toString()) === norm(quote)) r[key]++
          else r[key + 'Eg'] ??= [quote, got?.toString()?.slice(0, 40)]
        } catch (e) { r[key + 'Throw']++; r[key + 'Err'] ??= String(e).slice(0, 80) }
      }
      tryIt(koboFilter, 'filtered'); tryIt(undefined, 'unfiltered')
    }
  }
  await rep('LOC ' + JSON.stringify(r))
}

export async function stressTest(view, cfg, rep, inv) {
  let t0 = performance.now()
  await withTimeout(view.goTo(0), 60000).catch(e => rep('goTo0 ' + e))
  const first = Math.round(performance.now() - t0)
  await sleep(500)
  const c0 = view.renderer.getContents?.()[0]
  const doc = c0?.doc
  await rep(`STRESS ${cfg.id} first=${first}ms fxl=${view.isFixedLayout} rtl=${view.book.dir} url=${doc?.URL?.slice(0, 20)} textLen=${doc?.body?.textContent?.trim().length} wm=${doc ? doc.defaultView.getComputedStyle(doc.documentElement).writingMode : '-'} dir=${doc?.documentElement?.dir}`)
  await inv('spike_snapshot', { name: cfg.id + '-1' })
  const fr = [view.lastLocation?.fraction], times = []
  for (let k = 0; k < 5; k++) { t0 = performance.now(); await withTimeout(view.next(), 20000).catch(e => rep('next ' + e)); times.push(Math.round(performance.now() - t0)); await sleep(150); fr.push(view.lastLocation?.fraction) }
  await rep(`STRESS ${cfg.id} turns=${JSON.stringify(times)} fractions=${JSON.stringify(fr.map(f => f == null ? f : +f.toFixed(4)))} sections=${view.book.sections.length}`)
  await inv('spike_snapshot', { name: cfg.id + '-2' })
  if (cfg.id.includes('selfclosed') || cfg.id.includes('malformed')) await rep(`STRESS body=${JSON.stringify(doc?.body?.textContent?.trim().slice(0, 110))}`)
}
