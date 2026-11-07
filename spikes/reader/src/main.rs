use std::{collections::HashMap, fs::{self, OpenOptions}, io::{Read, Write}, sync::Mutex, time::Duration};
use tauri::{ipc::Response, Manager, Webview, WebviewUrl, WebviewWindowBuilder};
use zip::ZipArchive;

fn out(line: &str) {
    let path = std::env::var("SPIKE_OUT").unwrap_or("/dev/stdout".into());
    let mut f = OpenOptions::new().create(true).append(true).open(path).unwrap();
    writeln!(f, "{line}").unwrap();
}
fn book_path(id: &str) -> String { format!("{}/{id}", std::env::var("SPIKE_BOOKS").unwrap()) }
#[derive(Default)]
struct Open(Mutex<HashMap<String, ZipArchive<fs::File>>>);

#[tauri::command]
fn device_restore(webview: Webview) -> String {
    out(&format!("BACKEND device_restore RAN for label={}", webview.label()));
    "restored(stub)".into()
}
#[tauri::command]
fn reader_save_position(webview: Webview, pos: String) -> String {
    out(&format!("BACKEND reader_save_position label={} pos={pos}", webview.label()));
    "saved".into()
}
#[derive(serde::Serialize)]
struct Entry { name: String, size: u64 }
#[tauri::command]
fn reader_list(open: tauri::State<Open>, id: String) -> Result<Vec<Entry>, String> {
    let mut map = open.0.lock().unwrap();
    if !map.contains_key(&id) {
        let f = fs::File::open(book_path(&id)).map_err(|e| e.to_string())?;
        map.insert(id.clone(), ZipArchive::new(f).map_err(|e| e.to_string())?);
    }
    let z = map.get_mut(&id).unwrap();
    let mut v = vec![];
    for i in 0..z.len() { let e = z.by_index(i).map_err(|e| e.to_string())?; v.push(Entry { name: e.name().map_err(|e| e.to_string())?.into_owned(), size: e.size() }); }
    Ok(v)
}
#[tauri::command(async)]
fn reader_entry(open: tauri::State<'_, Open>, id: String, name: String) -> Result<Response, String> {
    let mut map = open.0.lock().unwrap();
    let z = map.get_mut(&id).ok_or("not open")?;
    let mut e = z.by_name(&name).map_err(|e| e.to_string())?;
    let mut b = Vec::with_capacity(e.size() as usize);
    e.read_to_end(&mut b).map_err(|e| e.to_string())?;
    Ok(Response::new(b))
}
#[tauri::command(async)]
fn reader_file(id: String) -> Result<Response, String> {
    Ok(Response::new(fs::read(book_path(&id)).map_err(|e| e.to_string())?))
}
#[tauri::command]
fn reader_config() -> serde_json::Value {
    serde_json::from_str(&std::env::var("SPIKE_CFG").unwrap_or("{}".into())).unwrap()
}
#[tauri::command]
fn reader_books() -> Vec<String> {
    std::env::var("SPIKE_LIST").unwrap().split(',').map(String::from).collect()
}
#[tauri::command]
fn reader_close(open: tauri::State<Open>, id: String) { open.0.lock().unwrap().remove(&id); }
#[tauri::command]
fn report(app: tauri::AppHandle, webview: Webview, msg: String) {
    out(&format!("[{}] {msg}", webview.label()));
    if msg == "DONE" { app.exit(0); }
}

#[allow(unexpected_cfgs)]
mod native {
    use objc2::{rc::Retained, runtime::AnyObject, MainThreadMarker};
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::{NSPoint, NSProcessInfo, NSString};
    use std::ffi::c_void;
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateScrollWheelEvent2(src: *const c_void, units: u32, count: u32, w1: i32, w2: i32, w3: i32) -> *mut c_void;
        fn CGEventSetLocation(e: *mut c_void, p: NSPoint);
        fn CGEventSetIntegerValueField(e: *mut c_void, f: u32, v: i64);
    }
    pub fn window(p: *mut c_void) -> Retained<NSWindow> { unsafe { Retained::retain(p as *mut NSWindow).unwrap() } }
    fn pt(w: &NSWindow, x: f64, y: f64) -> NSPoint {
        let h = w.contentLayoutRect().size.height;
        NSPoint::new(x, h - y)
    }
    pub fn mouse(w: &NSWindow, kind: NSEventType, x: f64, y: f64, clicks: isize) {
        let t = NSProcessInfo::processInfo().systemUptime();
        let e = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind, pt(w, x, y), NSEventModifierFlags::empty(), t, w.windowNumber(), None, 0, clicks, 1.0).unwrap();
        w.sendEvent(&e);
    }
    pub fn key(w: &NSWindow, code: u16, down: bool) {
        let t = NSProcessInfo::processInfo().systemUptime();
        let s = NSString::from_str("\u{F703}");
        let e = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            if down { NSEventType::KeyDown } else { NSEventType::KeyUp }, NSPoint::new(0., 0.), NSEventModifierFlags::empty(), t, w.windowNumber(), None, &s, &s, false, code).unwrap();
        w.sendEvent(&e);
    }
    pub fn wheel(w: &NSWindow, x: f64, y: f64, dy: i32) {
        let p = w.convertPointToScreen(pt(w, x, y));
        let mtm = MainThreadMarker::new().unwrap();
        let sh = objc2_app_kit::NSScreen::screens(mtm).objectAtIndex(0).frame().size.height;
        unsafe {
            let c = CGEventCreateScrollWheelEvent2(std::ptr::null(), 0, 1, dy, 0, 0);
            CGEventSetLocation(c, NSPoint::new(p.x, sh - p.y));
            CGEventSetIntegerValueField(c, 91, w.windowNumber() as i64);
            CGEventSetIntegerValueField(c, 92, w.windowNumber() as i64);
            let e: Option<Retained<NSEvent>> = objc2::msg_send![objc2::class!(NSEvent), eventWithCGEvent: c as *mut AnyObject];
            let e = e.unwrap();
            eprintln!("wheel loc {:?} win {:?}", e.locationInWindow(), e.windowNumber());
            w.sendEvent(&e);
        }
    }
    pub fn add_rules(wk: *mut c_void, done: impl Fn(String) + 'static) {
        use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore, WKWebView};
        use objc2_foundation::NSError;
        let wk: Retained<WKWebView> = unsafe { Retained::retain(wk as *mut WKWebView).unwrap() };
        let json = NSString::from_str(r#"[{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}}]"#);
        let id = NSString::from_str("reader-block-remote");
        let store = unsafe { WKContentRuleListStore::defaultStore(MainThreadMarker::new().unwrap()) }.unwrap();
        let wk2 = wk.clone();
        let block = block2::RcBlock::new(move |list: *mut WKContentRuleList, err: *mut NSError| {
            if let Some(list) = unsafe { list.as_ref() } {
                unsafe { wk2.configuration().userContentController().addContentRuleList(list) };
                done("RULES added".into());
            } else { done(format!("RULES failed {:?}", unsafe { err.as_ref() })); }
        });
        unsafe { store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(Some(&id), Some(&json), Some(&block)) };
    }
}

#[tauri::command(async)]
fn spike_snapshot(window: tauri::WebviewWindow, name: String) {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let dir = std::env::var("SPIKE_SHOTS").unwrap_or("/tmp".into());
    window.with_webview(move |pw| {
        use objc2::rc::Retained; use objc2_web_kit::WKWebView; use objc2_app_kit::NSImage; use objc2_foundation::NSError;
        let wk: Retained<WKWebView> = unsafe { Retained::retain(pw.inner() as *mut WKWebView).unwrap() };
        let path = format!("{dir}/{}.tiff", name.replace('/', "_"));
        let tx = tx.clone();
        let block = block2::RcBlock::new(move |img: *mut NSImage, _e: *mut NSError| {
            if let Some(img) = unsafe { img.as_ref() } {
                if let Some(d) = img.TIFFRepresentation() { std::fs::write(&path, d.to_vec()).ok(); }
            }
            tx.send(()).ok();
        });
        unsafe { wk.takeSnapshotWithConfiguration_completionHandler(None, &block) };
    }).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).ok();
}

#[tauri::command(async)]
fn spike_input(window: tauri::WebviewWindow, kind: String, x: f64, y: f64, x2: f64, y2: f64, key: u16) {
    use objc2_app_kit::NSEventType as T;
    let step = |f: Box<dyn FnOnce(&objc2_app_kit::NSWindow) + Send>| {
        window.with_webview(move |pw| { let w = native::window(pw.ns_window()); f(&w); }).unwrap();
        std::thread::sleep(Duration::from_millis(40));
    };
    match kind.as_str() {
        "click" | "dbl" => {
            step(Box::new(move |w| native::mouse(w, T::LeftMouseDown, x, y, 1)));
            step(Box::new(move |w| native::mouse(w, T::LeftMouseUp, x, y, 1)));
            if kind == "dbl" {
                step(Box::new(move |w| native::mouse(w, T::LeftMouseDown, x, y, 2)));
                step(Box::new(move |w| native::mouse(w, T::LeftMouseUp, x, y, 2)));
            }
        }
        "drag" => {
            step(Box::new(move |w| native::mouse(w, T::LeftMouseDown, x, y, 1)));
            for i in 1..=8 { let (px, py) = (x + (x2 - x) * i as f64 / 8., y + (y2 - y) * i as f64 / 8.);
                step(Box::new(move |w| native::mouse(w, T::LeftMouseDragged, px, py, 1))); }
            step(Box::new(move |w| native::mouse(w, T::LeftMouseUp, x2, y2, 1)));
        }
        "key" => { step(Box::new(move |w| native::key(w, key, true))); step(Box::new(move |w| native::key(w, key, false))); }
        "wheel" => { let dy = y2 as i32; step(Box::new(move |w| native::wheel(w, x, y, dy))); }
        _ => {}
    }
}

fn main() {
    let secs: u64 = std::env::var("SPIKE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
    let pages = std::env::var("SPIKE_PAGES").unwrap_or("main=main.html,reader-1=reader.html".into());
    let guard = std::env::var_os("SPIKE_GUARD").is_some();
    let mut ctx = tauri::generate_context!();
    if let Ok(csp) = std::env::var("SPIKE_CSP") {
        ctx.config_mut().app.security.csp = Some(tauri::utils::config::Csp::Policy(csp));
    }
    tauri::Builder::default()
        .manage(Open::default())
        .invoke_handler(tauri::generate_handler![device_restore, reader_list, reader_entry, reader_file, reader_config, reader_books, reader_close, reader_save_position, report, spike_input, spike_snapshot])
        .setup(move |app| {
            for spec in pages.split(',') {
                let (label, page) = spec.split_once('=').unwrap();
                let hdr = std::env::var("SPIKE_HDR").ok();
                WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
                    .on_web_resource_request(move |_req, resp| {
                        if let Some(h) = &hdr {
                            resp.headers_mut().insert("content-security-policy", h.parse().unwrap());
                        }
                    })
                    .on_navigation({ let l = label.to_string(); move |u| {
                        let s = u.scheme();
                        let ok = !guard || s == "tauri" || s == "blob" || u.as_str().starts_with("about:");
                        out(&format!("NAV {l} {} allow={ok}", &u.as_str()[..u.as_str().len().min(70)]));
                        ok } })
                    .on_new_window({ let l = label.to_string(); move |u, _f| {
                        out(&format!("NEWWIN {l} {} denied", &u.as_str()[..u.as_str().len().min(70)]));
                        tauri::webview::NewWindowResponse::Deny } })
                    .background_throttling(if std::env::var_os("SPIKE_THROTTLE").is_some() { tauri::utils::config::BackgroundThrottlingPolicy::Suspend } else { tauri::utils::config::BackgroundThrottlingPolicy::Disabled })
                    .title(label).inner_size(900.0, 700.0).incognito(label.starts_with("reader")).build()?;
            }
            if std::env::var_os("SPIKE_RULES").is_some() {
                if let Some(w) = app.get_webview_window("reader-1") {
                    w.with_webview(|pw| native::add_rules(pw.inner(), |m| out(&m))).unwrap();
                }
            }
            let h = app.handle().clone();
            std::thread::spawn(move || { std::thread::sleep(Duration::from_secs(secs)); out("END"); h.exit(0); });
            let _ = app.get_webview_window("main");
            Ok(())
        })
        .run(ctx)
        .expect("run");
}
