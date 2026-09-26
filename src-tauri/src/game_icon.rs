//! Puts the playing account's head on Minecraft's own window/taskbar entry.
//! The game sets its window icon itself and there's no launcher API for
//! changing it, so this reaches into the OS from outside: `_NET_WM_ICON` on
//! X11/XWayland, `WM_SETICON` on Windows. macOS has no equivalent, so it's a
//! no-op there. Purely cosmetic and best-effort - every failure is silent.

use crate::state::AppState;
use std::time::Duration;
use tauri::Manager;

/// Crops the face (plus the hat overlay layer) out of a skin PNG and scales
/// it up to a 64x64 RGBA icon.
pub fn skin_to_head_icon(png_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.width < 64 || info.height < 16 {
        return None;
    }
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        _ => return None,
    };
    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * info.width + x) as usize) * channels;
        [buf[i], buf[i + 1], buf[i + 2], if channels == 4 { buf[i + 3] } else { 255 }]
    };
    // Old 64x32 skins often fill the unused hat area with one opaque color;
    // Minecraft itself ignores a hat layer with no transparency at all, so
    // this does too (otherwise the "head" is just a flat square).
    let hat_has_transparency = (40..48).any(|x| (8..16).any(|y| pixel(x, y)[3] < 128));
    const SCALE: u32 = 8;
    let mut out = vec![0u8; (64 * 64 * 4) as usize];
    for y in 0..64u32 {
        for x in 0..64u32 {
            let (sx, sy) = (x / SCALE, y / SCALE);
            let face = pixel(8 + sx, 8 + sy);
            let hat = pixel(40 + sx, 8 + sy);
            let p = if hat_has_transparency && hat[3] > 0 { hat } else { face };
            let o = ((y * 64 + x) * 4) as usize;
            out[o..o + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
        }
    }
    Some(out)
}


/// 64x64 RGBA -> a 32x32 copy (every other pixel; the source is already
/// blocky nearest-neighbor art, so nothing is lost).
fn half_size(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32usize {
        for x in 0..32usize {
            let i = (y * 2 * 64 + x * 2) * 4;
            out.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    out
}

/// Fetches the account's head and applies it to the game's window. The
/// window doesn't exist for the first several seconds of a launch, so this
/// polls until it appears. Minecraft sets its own icon once, while creating
/// the window - if that lands just after ours it would win, so the head is
/// re-applied for a short while after the first success, then left alone.
pub async fn keep_applied(app: tauri::AppHandle, uuid: String, pid: u32) {
    const SETTLE_CHECKS: u32 = 12; // ~1 minute at 5s
    let state = app.state::<AppState>();
    let Some(url) = crate::minecraft::profile::fetch_public_skin_url(&state.http, &uuid).await else { return };
    let Some(bytes) = async { state.http.get(url).send().await.ok()?.bytes().await.ok() }.await else { return };
    let Some(rgba) = skin_to_head_icon(&bytes) else { return };
    let Ok(prepared) = tauri::async_runtime::spawn_blocking(move || platform::prepare(&rgba, &half_size(&rgba))).await
    else {
        return;
    };
    let Some(prepared) = prepared else { return };
    let prepared = std::sync::Arc::new(prepared);

    let mut checks_after_first = None::<u32>;
    loop {
        let still_running = state.running_instances.lock().await.values().any(|r| r.pid == pid);
        if !still_running {
            return;
        }
        let p = prepared.clone();
        let ok = tauri::async_runtime::spawn_blocking(move || platform::apply(&p, pid)).await.unwrap_or(false);
        if ok || checks_after_first.is_some() {
            let n = checks_after_first.get_or_insert(0);
            *n += 1;
            if *n > SETTLE_CHECKS {
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(if checks_after_first.is_some() { 5 } else { 2 })).await;
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode, Window};
    use x11rb::wrapper::ConnectionExt as _;

    pub struct Prepared {
        /// `_NET_WM_ICON` payload: for each size, width, height, then ARGB pixels.
        data: Vec<u32>,
    }

    fn argb(rgba: &[u8], size: u32, out: &mut Vec<u32>) {
        out.push(size);
        out.push(size);
        out.extend(rgba.chunks_exact(4).map(|p| {
            (u32::from(p[3]) << 24) | (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2])
        }));
    }

    pub fn prepare(large: &[u8], small: &[u8]) -> Option<Prepared> {
        let mut data = Vec::new();
        argb(large, 64, &mut data);
        argb(small, 32, &mut data);
        Some(Prepared { data })
    }

    fn atom(conn: &impl Connection, name: &str) -> Option<u32> {
        Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
    }

    pub fn apply(prepared: &Prepared, pid: u32) -> bool {
        let Ok((conn, screen)) = x11rb::connect(None) else { return false };
        let root = conn.setup().roots[screen].root;
        let (Some(client_list), Some(wm_pid), Some(wm_icon)) =
            (atom(&conn, "_NET_CLIENT_LIST"), atom(&conn, "_NET_WM_PID"), atom(&conn, "_NET_WM_ICON"))
        else {
            return false;
        };
        let Some(windows) = conn
            .get_property(false, root, client_list, AtomEnum::WINDOW, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return false;
        };
        let mut applied = false;
        for window in windows.value32().into_iter().flatten().collect::<Vec<Window>>() {
            let owner = conn
                .get_property(false, window, wm_pid, AtomEnum::CARDINAL, 0, 1)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().and_then(|mut v| v.next()));
            if owner == Some(pid)
                && conn
                    .change_property32(PropMode::REPLACE, window, wm_icon, AtomEnum::CARDINAL, &prepared.data)
                    .is_ok()
            {
                applied = true;
            }
        }
        // A round trip (not just a flush) so the writes have actually been
        // processed before this connection is dropped.
        let _ = conn.get_input_focus().ok().and_then(|c| c.reply().ok());
        applied
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateIcon, EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SendMessageW, ICON_BIG, ICON_SMALL,
        WM_SETICON,
    };

    pub struct Prepared {
        big: isize,
        small: isize,
    }

    fn make_icon(rgba: &[u8], size: i32) -> Option<isize> {
        // Windows wants BGRA for the color plane plus a (here all-zero, since
        // the alpha channel carries the shape) 1bpp AND mask.
        let mut bgra = Vec::with_capacity(rgba.len());
        for p in rgba.chunks_exact(4) {
            bgra.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        let mask = vec![0u8; (size as usize / 8) * size as usize];
        let icon = unsafe { CreateIcon(std::ptr::null_mut(), size, size, 1, 32, mask.as_ptr(), bgra.as_ptr()) };
        (!icon.is_null()).then_some(icon as isize)
    }

    pub fn prepare(large: &[u8], small: &[u8]) -> Option<Prepared> {
        Some(Prepared { big: make_icon(large, 64)?, small: make_icon(small, 32)? })
    }

    struct Search {
        pid: u32,
        found: Vec<HWND>,
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam as *mut Search);
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, &mut owner);
        if owner == search.pid && IsWindowVisible(hwnd) != 0 {
            search.found.push(hwnd);
        }
        1
    }

    pub fn apply(prepared: &Prepared, pid: u32) -> bool {
        let mut search = Search { pid, found: Vec::new() };
        unsafe {
            EnumWindows(Some(collect), &mut search as *mut Search as LPARAM);
            for &hwnd in &search.found {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, prepared.big);
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, prepared.small);
            }
        }
        !search.found.is_empty()
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    pub struct Prepared;
    pub fn prepare(_large: &[u8], _small: &[u8]) -> Option<Prepared> {
        None
    }
    pub fn apply(_prepared: &Prepared, _pid: u32) -> bool {
        false
    }
}
