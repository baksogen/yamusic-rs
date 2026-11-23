//! macOS: значок в строке меню — аналог трея Windows (src/tray.rs). Значок —
//! SF Symbol «music.note», шаблонный: сам меняет цвет под светлую и тёмную строку меню.

use super::objc::*;
use crate::i18n::tr;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::os::raw::c_long;

#[derive(Clone, Copy)]
pub enum Event {
    PlayPause,
    Prev,
    Next,
    Show,
    Quit,
}

/// Номер в массиве — tag пункта меню.
const EVENTS: [Event; 5] = [Event::PlayPause, Event::Prev, Event::Next, Event::Show, Event::Quit];

type Handler = Box<dyn Fn(Event)>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
    static PLAY_ITEM: Cell<Id> = const { Cell::new(std::ptr::null_mut()) };
    static PLAYING: Cell<Option<bool>> = const { Cell::new(None) };
}

extern "C" fn on_item(_s: Id, _sel: Sel, item: Id) {
    let tag: c_long = unsafe {
        let f: unsafe extern "C" fn(Id, Sel) -> c_long = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
        f(item, sel_registerName(c("tag").as_ptr()))
    };
    let Some(&e) = EVENTS.get(tag as usize) else { return };
    let _ = slint::invoke_from_event_loop(move || {
        HANDLER.with(|h| {
            if let Some(f) = h.borrow().as_ref() {
                f(e);
            }
        })
    });
}

unsafe fn add_item(menu: Id, target: Id, title: &str, tag: usize) -> Id {
    let ns = objc_getClass(c("NSString").as_ptr());
    let init: unsafe extern "C" fn(Id, Sel, Id, Sel, Id) -> Id =
        std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    let item = init(
        msg0(objc_getClass(c("NSMenuItem").as_ptr()), "alloc"),
        sel_registerName(c("initWithTitle:action:keyEquivalent:").as_ptr()),
        msg_string(ns, title),
        sel_registerName(c("menuItem:").as_ptr()),
        msg_string(ns, ""),
    );
    msg_set_id(item, "setTarget:", target);
    msg_set_long(item, "setTag:", tag as c_long);
    msg_set_id(menu, "addItem:", item);
    item
}

unsafe fn add_separator(menu: Id) {
    msg_set_id(menu, "addItem:", msg0(objc_getClass(c("NSMenuItem").as_ptr()), "separatorItem"));
}

/// Звать один раз из главного потока после создания окна.
pub fn install(on_event: impl Fn(Event) + 'static) {
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(on_event)));
    unsafe {
        let cls = objc_allocateClassPair(objc_getClass(c("NSObject").as_ptr()), c("YamusicStatusMenu").as_ptr(), 0);
        if cls.is_null() {
            crate::config::log("mac: класс меню не создался");
            return;
        }
        class_addMethod(cls, sel_registerName(c("menuItem:").as_ptr()), on_item as *const c_void, c("v@:@").as_ptr());
        objc_registerClassPair(cls);
        // Цель и статус-элемент живут до конца процесса: не освобождаем.
        let target = msg0(msg0(cls, "alloc"), "init");

        let menu = msg0(msg0(objc_getClass(c("NSMenu").as_ptr()), "alloc"), "init");
        PLAY_ITEM.set(add_item(menu, target, &tr("Воспроизведение"), 0));
        add_item(menu, target, &tr("Предыдущая песня"), 1);
        add_item(menu, target, &tr("Следующая песня"), 2);
        add_separator(menu);
        add_item(menu, target, &tr("Развернуть"), 3);
        add_separator(menu);
        add_item(menu, target, &tr("Закрыть приложение"), 4);

        let bar = msg0(objc_getClass(c("NSStatusBar").as_ptr()), "systemStatusBar");
        let status: Id = {
            let f: unsafe extern "C" fn(Id, Sel, f64) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
            f(bar, sel_registerName(c("statusItemWithLength:").as_ptr()), -1.0) // NSVariableStatusItemLength
        };
        // Строка меню не держит элемент сама.
        msg0(status, "retain");
        let button = msg0(status, "button");
        let ns = objc_getClass(c("NSString").as_ptr());
        let image: Id = {
            let f: unsafe extern "C" fn(Id, Sel, Id, Id) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
            f(
                objc_getClass(c("NSImage").as_ptr()),
                sel_registerName(c("imageWithSystemSymbolName:accessibilityDescription:").as_ptr()),
                msg_string(ns, "music.note"),
                msg_string(ns, env!("APP_NAME")),
            )
        };
        if !button.is_null() {
            if image.is_null() {
                msg_set_id(button, "setTitle:", msg_string(ns, "♫"));
            } else {
                msg_set_id(button, "setImage:", image);
            }
        }
        msg_set_id(status, "setMenu:", menu);
    }
}

/// Подпись первого пункта: «Пауза» или «Воспроизведение». Звать из sync_player.
pub fn set_playing(playing: bool) {
    if PLAYING.replace(Some(playing)) == Some(playing) {
        return;
    }
    let item = PLAY_ITEM.get();
    if item.is_null() {
        return;
    }
    unsafe {
        let ns = objc_getClass(c("NSString").as_ptr());
        msg_set_id(item, "setTitle:", msg_string(ns, &tr(if playing { "Пауза" } else { "Воспроизведение" })));
    }
}
