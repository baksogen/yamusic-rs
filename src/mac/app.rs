//! macOS: клик по значку в Dock возвращает спрятанное окно; ⌘Q, «Завершить» в Dock
//! и выход из системы закрывают приложение через наш quit(), с сохранением окна.
//! Методы добавляются в делегат winit (WinitApplicationDelegate): сам winit их не реализует.

use super::objc::*;
use std::cell::RefCell;
use std::ffi::c_void;

#[derive(Clone, Copy)]
pub enum Event {
    Reopen,
    Quit,
}

type Handler = Box<dyn Fn(Event)>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

/// Из обработчика AppKit в Slint не заходим: откладываем в цикл событий.
fn fire(e: Event) {
    let _ = slint::invoke_from_event_loop(move || {
        HANDLER.with(|h| {
            if let Some(f) = h.borrow().as_ref() {
                f(e);
            }
        })
    });
}

/// applicationShouldHandleReopen:hasVisibleWindows:
extern "C" fn should_reopen(_s: Id, _sel: Sel, _app: Id, _visible: bool) -> bool {
    fire(Event::Reopen);
    true
}

/// applicationShouldTerminate: — NSTerminateLater (2): система ждёт, а процесс
/// завершает gui::run после сохранения окна. Cancel сорвал бы выход из системы.
extern "C" fn should_terminate(_s: Id, _sel: Sel, _app: Id) -> usize {
    fire(Event::Quit);
    2
}

/// Звать из главного потока после создания окна: делегат winit уже установлен.
pub fn install(on_event: impl Fn(Event) + 'static) {
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(on_event)));
    unsafe {
        let cls = objc_getClass(c("WinitApplicationDelegate").as_ptr());
        if cls.is_null() {
            crate::config::log("mac: делегат winit не найден, Dock и ⌘Q без обработчиков");
            return;
        }
        class_addMethod(
            cls,
            sel_registerName(c("applicationShouldHandleReopen:hasVisibleWindows:").as_ptr()),
            should_reopen as *const c_void,
            c("B@:@B").as_ptr(),
        );
        class_addMethod(cls, sel_registerName(c("applicationShouldTerminate:").as_ptr()), should_terminate as *const c_void, c("Q@:@").as_ptr());
        // NSApplication запоминает методы делегата при setDelegate: — переустанавливаем.
        // Делегат держит winit, сброс в nil его не освобождает.
        let app = msg0(objc_getClass(c("NSApplication").as_ptr()), "sharedApplication");
        let delegate = msg0(app, "delegate");
        msg_set_id(app, "setDelegate:", std::ptr::null_mut());
        msg_set_id(app, "setDelegate:", delegate);
    }
}
