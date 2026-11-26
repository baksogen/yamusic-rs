//! Обложка для карточки «Сейчас играет». Без неё macOS рисует маленький значок приложения в
//! большой рамке. MPMediaItemArtwork на macOS принимает только блок `^NSImage *(CGSize)`,
//! поэтому блок собираем вручную по ABI Blocks: стековый, с copy/dispose, которые держат картинку.

use super::objc::*;
use crate::api::Track;
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_void};
use std::os::raw::c_ulong;
use std::sync::OnceLock;
use std::time::Duration;

extern "C" {
    static _NSConcreteStackBlock: [*const c_void; 32];
    fn objc_retain(obj: Id) -> Id;
    fn objc_release(obj: Id);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    w: f64,
    h: f64,
}

#[repr(C)]
struct Block {
    isa: *const c_void,
    flags: c_int,
    reserved: c_int,
    invoke: unsafe extern "C" fn(*mut Block, CGSize) -> Id,
    descriptor: *const Descriptor,
    image: Id,
}

#[repr(C)]
struct Descriptor {
    reserved: c_ulong,
    size: c_ulong,
    copy: unsafe extern "C" fn(*mut Block, *const Block),
    dispose: unsafe extern "C" fn(*mut Block),
    signature: *const c_char,
}

// Только константы и указатели на функции и статическую строку.
unsafe impl Sync for Descriptor {}

const BLOCK_HAS_COPY_DISPOSE: c_int = 1 << 25;
const BLOCK_HAS_SIGNATURE: c_int = 1 << 30;

static DESCRIPTOR: Descriptor = Descriptor {
    reserved: 0,
    size: std::mem::size_of::<Block>() as c_ulong,
    copy: block_copy,
    dispose: block_dispose,
    signature: c"@24@?0{CGSize=dd}8".as_ptr(),
};

// Блок может позвать любой поток; картинку он держит сам, так что она жива, пока жив блок.
unsafe extern "C" fn block_invoke(b: *mut Block, _: CGSize) -> Id {
    (*b).image
}

// Block_copy уже скопировал поля, остаётся взять свою ссылку на картинку.
unsafe extern "C" fn block_copy(dst: *mut Block, _src: *const Block) {
    objc_retain((*dst).image);
}

unsafe extern "C" fn block_dispose(b: *mut Block) {
    objc_release((*b).image);
}

/// Обложка текущего трека: адрес и готовая MPMediaItemArtwork (null, пока качается).
struct Art {
    url: String,
    art: Id,
}

impl Drop for Art {
    fn drop(&mut self) {
        if !self.art.is_null() {
            unsafe { objc_release(self.art) };
        }
    }
}

thread_local! {
    static ART: RefCell<Option<Art>> = const { RefCell::new(None) };
}

/// Готовая обложка трека, если уже скачана.
pub(super) fn current(t: &Track) -> Option<Id> {
    let url = t.cover_url(400)?;
    ART.with(|a| a.borrow().as_ref().filter(|a| a.url == url && !a.art.is_null()).map(|a| a.art))
}

/// Начать качать обложку трека, если её ещё не качали.
pub(super) fn want(t: &Track) {
    let Some(url) = t.cover_url(400) else {
        ART.with(|a| a.borrow_mut().take());
        return;
    };
    if ART.with(|a| a.borrow().as_ref().is_some_and(|a| a.url == url)) {
        return;
    }
    ART.with(|a| *a.borrow_mut() = Some(Art { url: url.clone(), art: std::ptr::null_mut() }));
    std::thread::spawn(move || {
        static HTTP: OnceLock<reqwest::blocking::Client> = OnceLock::new();
        let http = HTTP.get_or_init(|| reqwest::blocking::Client::builder().timeout(Duration::from_secs(20)).build().expect("http client"));
        match http.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()) {
            Ok(bytes) => {
                let _ = slint::invoke_from_event_loop(move || ready(&url, &bytes));
            }
            Err(e) => crate::config::log(format!("media: обложка не скачалась: {e}")),
        }
    });
}

/// Обложка скачалась (главный поток): собрать MPMediaItemArtwork, если трек ещё тот же.
fn ready(url: &str, bytes: &[u8]) {
    if !ART.with(|a| a.borrow().as_ref().is_some_and(|a| a.url == url)) {
        return;
    }
    unsafe {
        let data = msg_data(objc_getClass(c("NSData").as_ptr()), bytes);
        let image = msg_init_with(msg0(objc_getClass(c("NSImage").as_ptr()), "alloc"), "initWithData:", data);
        if image.is_null() {
            crate::config::log("media: обложка не распозналась");
            return;
        }
        let mut block = Block {
            isa: std::ptr::addr_of!(_NSConcreteStackBlock).cast(),
            flags: BLOCK_HAS_COPY_DISPOSE | BLOCK_HAS_SIGNATURE,
            reserved: 0,
            invoke: block_invoke,
            descriptor: &DESCRIPTOR,
            image,
        };
        let class = objc_getClass(c("MPMediaItemArtwork").as_ptr());
        let init: unsafe extern "C" fn(Id, Sel, CGSize, *mut Block) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
        let art = init(msg0(class, "alloc"), sel_registerName(c("initWithBoundsSize:requestHandler:").as_ptr()), CGSize { w: 400.0, h: 400.0 }, &mut block);
        // Свою ссылку отдаём: картинку держит копия блока внутри MPMediaItemArtwork.
        objc_release(image);
        if art.is_null() {
            return;
        }
        ART.with(|a| match a.borrow_mut().as_mut() {
            Some(a) if a.url == url => a.art = art,
            _ => objc_release(art),
        });
    }
    super::media::refresh();
}

unsafe fn msg_data(cls: Id, bytes: &[u8]) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *const c_void, c_ulong) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(cls, sel_registerName(c("dataWithBytes:length:").as_ptr()), bytes.as_ptr().cast(), bytes.len() as c_ulong)
}

unsafe fn msg_init_with(obj: Id, sel: &str, arg: Id) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()), arg)
}
