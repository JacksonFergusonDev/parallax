import './foliate/view.js'
import { EPUB } from './foliate/epub.js'
import { runTests, surveyBook, locatorBook } from './tests.js'
const inv = window.__TAURI_INTERNALS__.invoke
export const rep = m => inv('report', { msg: String(m) }).catch(() => {})
window.addEventListener('error', e => rep('PAGE ERROR ' + e.message))
window.addEventListener('unhandledrejection', e => rep('REJECTION ' + (e.reason?.stack ?? e.reason)))
document.addEventListener('securitypolicyviolation', e => rep('CSPV parent ' + e.effectiveDirective + ' ' + e.blockedURI))
const dec = new TextDecoder()
const entryLoader = async id => {
  const list = await inv('reader_list', { id })
  const sizes = new Map(list.map(e => [e.name, e.size]))
  const bytes = name => inv('reader_entry', { id, name })
  return {
    entries: list.map(e => ({ filename: e.name })),
    getSize: n => sizes.get(n) ?? 0,
    loadText: async n => sizes.has(n) ? dec.decode(await bytes(n)) : null,
    loadBlob: async (n, type) => sizes.has(n) ? new Blob([await bytes(n)], { type }) : null,
  }
}
const cfg = await inv('reader_config')
await rep('reader boot cfg=' + JSON.stringify(cfg) + ' origin=' + location.origin)

const makeView = () => {
  const v = document.createElement('foliate-view')
  v.style.cssText = 'display:block;position:fixed;inset:0'
  document.body.append(v)
  return v
}
const openBook = async (view, id, mode) => {
  const t0 = performance.now()
  let book
  if (mode === 'entry') book = await new EPUB(await entryLoader(id)).init()
  else { const buf = await inv('reader_file', { id }); book = new File([buf], 'b.epub', { type: 'application/epub+zip' }) }
  await view.open(book)
  return Math.round(performance.now() - t0)
}
if (cfg.test === 'locator') {
  const ids = await inv('reader_books')
  for (const pair of ids) {
    const [plain, kep] = pair.split('|')
    try {
      const a = await new EPUB(await entryLoader(plain)).init(), b = await new EPUB(await entryLoader(kep)).init()
      await locatorBook(a, b, plain, rep)
    } catch (e) { await rep('LOCFAIL ' + plain + ' ' + String(e?.stack ?? e).slice(0, 240)) }
    await inv('reader_close', { id: plain }).catch(() => {}); await inv('reader_close', { id: kep }).catch(() => {})
  }
  await rep('DONE')
} else if (cfg.test === 'survey') {
  const ids = await inv('reader_books')
  for (const id of ids) {
    const view = makeView()
    try {
      const ms = await Promise.race([openBook(view, id, 'entry'), new Promise((_, r) => setTimeout(() => r(new Error('open timeout')), 20000))])
      await rep(`OPEN ${id} ${ms}ms`)
      let tm; await Promise.race([surveyBook(view, id, rep), new Promise(r => { tm = setTimeout(() => { rep('BOOK-TIMEOUT ' + id); r() }, 240000) })]); clearTimeout(tm)
    } catch (e) { await rep('OPENFAIL ' + id + ' ' + String(e?.stack ?? e).slice(0, 300)) }
    try { view.close?.() } catch {}
    view.remove()
    await inv('reader_close', { id }).catch(() => {})
  }
  await rep('DONE')
} else {
  const view = makeView()
  view.addEventListener('load', e => { e.detail.doc.addEventListener('securitypolicyviolation', v => rep('CSPV frame ' + v.effectiveDirective + ' ' + v.blockedURI.slice(0, 70))) })
  const ms = await openBook(view, cfg.id, cfg.mode)
  await rep(`opened in ${ms}ms sections=${view.book.sections.length} title=${JSON.stringify(view.book.metadata?.title)}`)
  window.view = view
  await runTests(view, cfg, rep, inv)
}
