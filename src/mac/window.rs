//! macOS: нативный «светофор», прозрачный заголовок и скругление окна winit,
//! рабочая область экрана для мини-плеера.

use super::objc::*;
use std::ffi::c_void;
use std::os::raw::{c_long, c_ulong};


/// Наблюдатель окна (init_window_observers) — цель отложенных performSelector.
static OBSERVER: std::sync::atomic::AtomicPtr<c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
// ---------- нативные кнопки окна ----------

/// Вернуть окну нативный вид: «светофор», прозрачный тайтлбар на всю высоту
/// (как `titleBarStyle: hiddenInset` у Electron-оригинала) и скруглённые углы.
/// Маску не форсируем: ею владеет winit (Slint no-frame:false → decorations:true),
/// добавляем только бит FullSizeContentView — winit set_decorations early-return'ится
/// при неизменном флаге и наш бит не стирает; fullscreen save/restore маски winit
/// делает сам и сохраняет бит. Форс полной маски вызывал цикл сбросов с winit.
unsafe fn restyle_window(win: Id) {
    if win.is_null() {
        return;
    }
    let gm: unsafe extern "C" fn(Id, Sel) -> c_ulong = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    let mask = gm(win, sel_registerName(c("styleMask").as_ptr()));
    msg_set_ulong(win, "setStyleMask:", mask | (1 << 15));
    msg_set_bool(win, "setTitlebarAppearsTransparent:", true);
    msg_set_long(win, "setTitleVisibility:", 1); // NSWindowTitleHidden
    // Сервер скругляет стандартные окна сам, но окно winit создаётся borderless,
    // поэтому задаём радиус самому окну. 24pt — как у Finder; нативный inset
    // светофора (16,16) при таком радиусе вжимается в дугу угла, поэтому кнопки
    // ставим вручную с inset Finder (26,26) — см. place_traffic_lights.
    let corner_sel = sel_registerName(c("_setCornerRadius:").as_ptr());
    if msg_responds(win, corner_sel) {
        msg_set_double(win, "_setCornerRadius:", 24.0);
    }
    place_traffic_lights(win);
}

/// Finder-inset: центры кнопок (26,26), (49,26), (72,26) от левого верхнего угла.
/// Нативный дефолт (16,16) при радиусе 24pt визуально вжат в дугу угла; шапки
/// страниц сдвинуты на App.chrome-top, чтобы кнопки не перехватывали «назад».
/// Трогаем только frame кнопок: маску и контейнер тайтлбара владеют winit/AppKit
/// (пересборка контейнера и форс маски в прошлых экспериментах ломали кнопки).
/// Origin пересчитываем из координат окна через convertPoint:fromView:nil —
/// не зависит от flip контейнера; высота окна нужна, т.к. Cocoa считает снизу.
const LIGHT_CX: [f64; 3] = [26.0, 49.0, 72.0];
const LIGHT_CY: f64 = 26.0;
/// Мини-плеер (окно 400×88): светофор перекрывал бы обложку — прячем. Флаг читает
/// place_traffic_lights, чтобы resize-observer не возвращал кнопки в мини.
static MINI: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe fn place_traffic_lights(win: Id) {
    if win.is_null() || !msg_responds(win, sel_registerName(c("standardWindowButton:").as_ptr())) {
        return;
    }
    let get: unsafe extern "C" fn(Id, Sel, c_long) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    let sh: unsafe extern "C" fn(Id, Sel, bool) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    let sa: unsafe extern "C" fn(Id, Sel, f64) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    let fo: unsafe extern "C" fn(Id, Sel, NsPoint) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    let cv: unsafe extern "C" fn(Id, Sel, NsPoint, Id) -> NsPoint = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    let mini = MINI.load(std::sync::atomic::Ordering::Relaxed);
    let h_win = ns_rect(win, "frame").h;
    for idx in 0..3 {
        let btn = get(win, sel_registerName(c("standardWindowButton:").as_ptr()), idx);
        if btn.is_null() {
            return;
        }
        sh(btn, sel_registerName(c("setHidden:").as_ptr()), mini);
        sa(btn, sel_registerName(c("setAlphaValue:").as_ptr()), 1.0);
        if mini {
            continue;
        }
        let container = msg0(btn, "superview");
        if container.is_null() {
            continue;
        }
        let f = ns_rect(btn, "frame");
        let p = NsPoint { x: LIGHT_CX[idx as usize] - f.w / 2.0, y: h_win - LIGHT_CY - f.h / 2.0 };
        let q = cv(container, sel_registerName(c("convertPoint:fromView:").as_ptr()), p, std::ptr::null_mut());
        fo(btn, sel_registerName(c("setFrameOrigin:").as_ptr()), q);
    }
}

/// Видимость светофора из состояния мини-плеера (App.mini): скрыть/показать и
/// переставить кнопки. `ns_view` — NSView окна winit, главный поток.
pub fn set_traffic_lights_hidden(ns_view: Id, hidden: bool) {
    MINI.store(hidden, std::sync::atomic::Ordering::Relaxed);
    unsafe {
        place_traffic_lights(msg0(ns_view, "window"));
    }
}

/// Первичная стилизация окна. `ns_view` — NSView окна winit; вызывать из главного
/// потока после появления winit-окна.
pub fn native_window_buttons(ns_view: Id) {
    unsafe {
        let win = msg0(ns_view, "window");
        restyle_window(win);
        // Страховка от финального лейаута AppKit после показа окна: повторная
        // постановка через 0.6 с (тот же приём, что после выхода из fullscreen).
        let observer = OBSERVER.load(std::sync::atomic::Ordering::Relaxed);
        if !observer.is_null() && !win.is_null() {
            let f: unsafe extern "C" fn(Id, Sel, Sel, Id, f64) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
            f(
                observer,
                sel_registerName(c("performSelector:withObject:afterDelay:").as_ptr()),
                sel_registerName(c("placeNow:").as_ptr()),
                win,
                0.6,
            );
        }
    }
}

/// После выхода из полноэкранного режима macOS/winit сбрасывают styleMask и
/// «светофор» пропадает — ловим уведомление и возвращаем нативный вид.
/// Вызывать один раз. Restyle только здесь и при init: на resize/becomeKey
/// нельзя — setStyleMask провоцирует winit переставить свою borderless-маску,
/// и возникает цикл сбросов, в котором кнопки исчезают.
pub fn init_window_observers() {
    unsafe {
        let cls = objc_allocateClassPair(objc_getClass(c("NSObject").as_ptr()), c("YamusicWindowObserver").as_ptr(), 0);
        if cls.is_null() {
            return;
        }
        let add = |name: &str, imp: extern "C" fn(Id, Sel, Id)| {
            class_addMethod(cls, sel_registerName(c(name).as_ptr()), imp as *const c_void, c("v@:@").as_ptr());
        };
        add("windowDidExitFullScreen:", on_exit_fullscreen);
        add("windowDidResize:", on_window_resize);
        add("windowDidBecomeKey:", on_window_resize);
        add("windowDidBecomeMain:", on_window_resize);
        add("restyleNow:", on_restyle_now);
        add("placeNow:", on_place_now);
        objc_registerClassPair(cls);
        let observer = msg0(msg0(cls, "alloc"), "init");
        let center = msg0(objc_getClass(c("NSNotificationCenter").as_ptr()), "defaultCenter");
        if observer.is_null() || center.is_null() {
            return;
        }
        OBSERVER.store(observer, std::sync::atomic::Ordering::Relaxed);
        let name = msg_string(objc_getClass(c("NSString").as_ptr()), "NSWindowDidExitFullScreenNotification");
        // addObserver:selector:name:object: — object nil, то есть все окна приложения.
        let f: unsafe extern "C" fn(Id, Sel, Id, Sel, Id, Id) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
        f(
            center,
            sel_registerName(c("addObserver:selector:name:object:").as_ptr()),
            observer,
            sel_registerName(c("windowDidExitFullScreen:").as_ptr()),
            name,
            std::ptr::null_mut(),
        );
        // Live resize: AppKit временно ставит кнопки в дефолт — возвращаем inset
        // синхронно. Только frame кнопок: маску не трогаем (её владелец winit).
        let rname = msg_string(objc_getClass(c("NSString").as_ptr()), "NSWindowDidResizeNotification");
        f(
            center,
            sel_registerName(c("addObserver:selector:name:object:").as_ptr()),
            observer,
            sel_registerName(c("windowDidResize:").as_ptr()),
            rname,
            std::ptr::null_mut(),
        );
    }
}

extern "C" fn on_exit_fullscreen(_self: Id, _sel: Sel, note: Id) {
    unsafe {
        let win = msg0(note, "object");
        // После анимации выхода (~0.5 с): AppKit сбрасывает frame кнопок в
        // своём лейауте во время перехода, наша постановка — после него.
        let f: unsafe extern "C" fn(Id, Sel, Sel, Id, f64) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
        f(
            _self,
            sel_registerName(c("performSelector:withObject:afterDelay:").as_ptr()),
            sel_registerName(c("restyleNow:").as_ptr()),
            win,
            0.6,
        );
    }
}

extern "C" fn on_restyle_now(_self: Id, _sel: Sel, win: Id) {
    unsafe {
        crate::config::log("window: restyle after fullscreen exit");
        restyle_window(win);
    }
}

/// Отложенная постановка кнопок: финальный лейаут тайтлбара AppKit (показ окна,
/// becomeKey) идёт после нашего init и сбрасывает frame в дефолт.
extern "C" fn on_place_now(_self: Id, _sel: Sel, win: Id) {
    unsafe {
        place_traffic_lights(win);
    }
}

/// Live resize: возвращаем inset кнопок синхронно (AppKit на время drag'а
/// ставит их в дефолт). Маску/геометрию контейнера не трогаем.
extern "C" fn on_window_resize(_self: Id, _sel: Sel, note: Id) {
    unsafe {
        place_traffic_lights(msg0(note, "object"));
    }
}

// ---------- рабочая область экрана (для мини-плеера) ----------

/// NSRect: origin{x,y} + size{w,h} — 4 CGFloat (f64 на arm64).
#[repr(C)]
struct NsRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// NSPoint: 2 CGFloat (HFA: передача/возврат в d0/d1 на arm64).
#[repr(C)]
struct NsPoint {
    x: f64,
    y: f64,
}

/// NSRect (32 байта) на x86_64 возвращается через скрытый указатель: нужен objc_msgSend_stret.
/// На arm64 он уходит в регистры d0–d3, поэтому там хватает objc_msgSend.
unsafe fn ns_rect(obj: Id, sel: &str) -> NsRect {
    let s = sel_registerName(c(sel).as_ptr());
    #[cfg(target_arch = "x86_64")]
    {
        let mut r = std::mem::MaybeUninit::<NsRect>::uninit();
        let f: unsafe extern "C" fn(*mut NsRect, Id, Sel) = std::mem::transmute(objc_msgSend_stret as unsafe extern "C" fn() as usize);
        f(r.as_mut_ptr(), obj, s);
        r.assume_init()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let f: unsafe extern "C" fn(Id, Sel) -> NsRect = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
        f(obj, s)
    }
}

/// Рабочая область основного экрана в физических пикселях, начало — левый верхний
/// угол основного экрана (как PhysicalPosition у winit): лево, верх, право, низ.
/// `visibleFrame` в Cocoa — в точках и с началом в левом нижнем углу, пересчитываем.
pub fn work_area() -> (i32, i32, i32, i32) {
    unsafe {
        let screen = msg0(objc_getClass(c("NSScreen").as_ptr()), "mainScreen");
        if screen.is_null() {
            return (0, 0, 1920, 1080);
        }
        let vf = ns_rect(screen, "visibleFrame");
        let k = msg_double(screen, "backingScaleFactor");
        // Высота основного экрана в точках: frame первой из [NSScreen screens].
        let screens = msg0(objc_getClass(c("NSScreen").as_ptr()), "screens");
        let first = if screens.is_null() { std::ptr::null_mut() } else { msg0(screens, "firstObject") };
        let h_pts = if first.is_null() { vf.y + vf.h } else { ns_rect(first, "frame").h };
        let left = (vf.x * k) as i32;
        let right = ((vf.x + vf.w) * k) as i32;
        let top = ((h_pts - (vf.y + vf.h)) * k) as i32;
        let bottom = ((h_pts - vf.y) * k) as i32;
        (left, top, right, bottom)
    }
}
