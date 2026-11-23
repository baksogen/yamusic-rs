//! macOS: медиаклавиши (F7/F8/F9) и карточка «Сейчас играет» в Пункте управления —
//! MPRemoteCommandCenter и MPNowPlayingInfoCenter из MediaPlayer.framework.

use super::objc::*;
use crate::api::Track;
use crate::player::{Cmd, Player};
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_void};
use std::os::raw::{c_long, c_ulong};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
}

/// Убедиться, что MediaPlayer.framework загружен (для objc_getClass его классов).
/// Фреймворк слинкован, но на всякий случай грузим явно — dlopen идемпотентен.
fn ensure_media_player() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| unsafe {
        let h = dlopen(c("/System/Library/Frameworks/MediaPlayer.framework/MediaPlayer").as_ptr(), 2 /* RTLD_NOW */);
        if h.is_null() {
            crate::config::log("media: MediaPlayer.framework не загрузился");
        }
    });
}

unsafe fn msg_target_action(obj: Id, sel: &str, target: Id, action: Sel) {
    let f: unsafe extern "C" fn(Id, Sel, Id, Sel) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()), target, action);
}

unsafe fn msg_number(cls: Id, v: f64) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, f64) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(cls, sel_registerName(c("numberWithDouble:").as_ptr()), v)
}

unsafe fn msg_dict(cls: Id, objects: *const Id, keys: *const Id, count: c_ulong) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *const Id, *const Id, c_ulong) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(cls, sel_registerName(c("dictionaryWithObjects:forKeys:count:").as_ptr()), objects, keys, count)
}

// ---------- медиа-клавиши ----------

static PLAYER: OnceLock<Player> = OnceLock::new();

fn player() -> Option<&'static Player> {
    PLAYER.get()
}

// Обработчики команд MediaPlayer обязаны возвращать MPRemoteCommandHandlerStatus
// (NSInteger): 0 = Success. Сигнатура метода в objc — "q@:@", не "v@:@".
const STATUS_SUCCESS: c_long = 0;

extern "C" fn on_play(_self: Id, _sel: Sel, _cmd: Id) -> c_long {
    if let Some(p) = player() {
        p.send(Cmd::Resume);
    }
    STATUS_SUCCESS
}

extern "C" fn on_pause(_self: Id, _sel: Sel, _cmd: Id) -> c_long {
    if let Some(p) = player() {
        if p.state().playing {
            p.send(Cmd::PlayPause);
        }
    }
    STATUS_SUCCESS
}

extern "C" fn on_toggle(_self: Id, _sel: Sel, _cmd: Id) -> c_long {
    if let Some(p) = player() {
        p.send(Cmd::PlayPause);
    }
    STATUS_SUCCESS
}

extern "C" fn on_next(_self: Id, _sel: Sel, _cmd: Id) -> c_long {
    if let Some(p) = player() {
        p.send(Cmd::Next);
    }
    STATUS_SUCCESS
}

extern "C" fn on_prev(_self: Id, _sel: Sel, _cmd: Id) -> c_long {
    if let Some(p) = player() {
        p.send(Cmd::Prev);
    }
    STATUS_SUCCESS
}

extern "C" fn on_seek(_self: Id, _sel: Sel, event: Id) -> c_long {
    // MPChangePlaybackPositionCommandEvent.positionTime — NSTimeInterval (double), не объект.
    let pos = unsafe { msg_double(event, "positionTime") };
    if let Some(p) = player() {
        p.send(Cmd::Seek(pos as f32));
    }
    STATUS_SUCCESS
}

/// Зарегистрировать обработчики медиа-клавиш и команд Пункта управления.
/// Вызывать один раз из главного потока после создания плеера.
pub fn init_media_keys(p: Player) {
    let _ = PLAYER.set(p);
    ensure_media_player();
    unsafe {
        let cls = objc_allocateClassPair(objc_getClass(c("NSObject").as_ptr()), c("YamusicMediaKeys").as_ptr(), 0);
        if cls.is_null() {
            crate::config::log("media: класс не создался");
            return;
        }
        let add = |name: &str, imp: extern "C" fn(Id, Sel, Id) -> c_long| {
            class_addMethod(
                cls,
                sel_registerName(c(name).as_ptr()),
                imp as *const c_void,
                c("q@:@").as_ptr(),
            );
        };
        add("play:", on_play);
        add("pause:", on_pause);
        add("togglePlayPause:", on_toggle);
        add("nextTrack:", on_next);
        add("previousTrack:", on_prev);
        add("changePlaybackPosition:", on_seek);
        objc_registerClassPair(cls);

        let target = msg0(msg0(cls, "alloc"), "init");
        let center = msg0(objc_getClass(c("MPRemoteCommandCenter").as_ptr()), "sharedCommandCenter");
        if target.is_null() || center.is_null() {
            crate::config::log("media: не удалось получить центр команд");
            return;
        }
        for (command, action) in [
            ("playCommand", "play:"),
            ("pauseCommand", "pause:"),
            ("togglePlayPauseCommand", "togglePlayPause:"),
            ("nextTrackCommand", "nextTrack:"),
            ("previousTrackCommand", "previousTrack:"),
            ("changePlaybackPositionCommand", "changePlaybackPosition:"),
        ] {
            let cmd = msg0(center, command);
            if !cmd.is_null() {
                msg_target_action(cmd, "addTarget:action:", target, sel_registerName(c(action).as_ptr()));
            }
        }
    }
}

// ---------- «Сейчас играет» ----------

struct Last {
    key: String,
    at: Instant,
}

thread_local! {
    static LAST: RefCell<Option<Last>> = const { RefCell::new(None) };
}

/// Обновить карточку в Пункте управления. Звать из sync_player (главный поток).
/// Пересобираем словарь при смене трека/состояния и раз в 2 секунды для позиции.
pub fn now_playing(track: Option<&Track>, playing: bool, position: f32, duration: f32) {
    unsafe {
        let center = msg0(objc_getClass(c("MPNowPlayingInfoCenter").as_ptr()), "defaultCenter");
        if center.is_null() {
            return;
        }
        let Some(t) = track else {
            if LAST.with(|l| l.borrow_mut().take()).is_some() {
                msg_set_id(center, "setNowPlayingInfo:", std::ptr::null_mut());
                msg_set_ulong(center, "setPlaybackState:", 3);
            }
            return;
        };
        let key = format!("{}\u{1}{}\u{1}{}\u{1}{duration}\u{1}{playing}", t.title, t.artists, t.album_title);
        let fresh = LAST.with(|l| {
            let mut l = l.borrow_mut();
            match l.as_ref() {
                Some(last) if last.key == key && last.at.elapsed() < Duration::from_secs(2) => false,
                _ => {
                    *l = Some(Last { key, at: Instant::now() });
                    true
                }
            }
        });
        if !fresh {
            return;
        }
        let nsstring = objc_getClass(c("NSString").as_ptr());
        let nsnumber = objc_getClass(c("NSNumber").as_ptr());
        // Ключи nowPlayingInfo — строковые значения констант MPMediaItemProperty* /
        // MPNowPlayingInfoProperty* (создаём сами, чтобы не зависеть от extern-символов).
        let keys = [
            msg_string(nsstring, "title"),
            msg_string(nsstring, "artist"),
            msg_string(nsstring, "albumTitle"),
            msg_string(nsstring, "playbackDuration"),
            // У MPNowPlayingInfoProperty* значение константы — полное имя строкой
            // (в отличие от MPMediaItem*, где короткое «title», «artist»).
            msg_string(nsstring, "MPNowPlayingInfoPropertyElapsedPlaybackTime"),
            msg_string(nsstring, "MPNowPlayingInfoPropertyPlaybackRate"),
        ];
        let values = [
            msg_string(nsstring, &t.title),
            msg_string(nsstring, &t.artists),
            msg_string(nsstring, &t.album_title),
            msg_number(nsnumber, duration as f64),
            msg_number(nsnumber, position as f64),
            msg_number(nsnumber, if playing { 1.0 } else { 0.0 }),
        ];
        let dict = msg_dict(objc_getClass(c("NSDictionary").as_ptr()), values.as_ptr(), keys.as_ptr(), keys.len() as c_ulong);
        msg_set_id(center, "setNowPlayingInfo:", dict);
        msg_set_ulong(center, "setPlaybackState:", if playing { 1 } else { 2 });
    }
}
