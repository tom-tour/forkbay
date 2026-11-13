// Settings applied to a game while it runs, on Windows, whether NX2 Launcher or the player started the emulator.
//
// During a game an emulator reads again only that game's own file, and only from its "Configure Current Game"
// window. So NX2 Launcher writes the setting there, presses that window's shortcut (F13, set by
// config::prepare_emulators) once the player is back in the game, and presses the window's OK button through
// UI Automation. tools/nx2-quiet keeps the window invisible and the game in front meanwhile.
// Elsewhere, and when anything fails, settings are applied as before: when the emulator closes or the game starts.

/// Asks for the settings just saved to be applied to the running game (only to `game`, when given).
/// Returns whether they will be.
#[cfg(not(windows))]
pub fn request(_game: Option<&str>) -> bool {
    false
}

#[cfg(windows)]
pub use imp::{request, wait};

/// Waits for the settings asked for to be applied, or given up (test mode, which ends right after).
#[cfg(not(windows))]
pub fn wait(_limit: std::time::Duration) {}

#[cfg(windows)]
mod imp {
    use crate::core::*;
    use anyhow::{anyhow, bail, Result};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0};
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, OpenProcess, QueryFullProcessImageNameW, ResetEvent, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, VK_F13};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW,
        PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, EVENT_OBJECT_CREATE, GW_OWNER, HHOOK, WH_GETMESSAGE, WINEVENT_INCONTEXT,
        WM_CLOSE, WM_KEYDOWN, WM_KEYUP, WM_NULL,
    };

    const DLL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nx2-quiet.dll"));

    static BUSY: AtomicBool = AtomicBool::new(false);
    static AGAIN: AtomicBool = AtomicBool::new(false);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// The emulator showing a game: its main window, its process and the game's program id.
    struct Target {
        pid: u32,
        main: HWND,
        id: String,
    }

    fn pid_of(w: HWND) -> u32 {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(w, &mut pid) };
        pid
    }

    fn exe_of(pid: u32) -> Option<String> {
        unsafe {
            let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if p.is_null() { return None; }
            let mut buf = [0u16; 1024];
            let mut n = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(p, 0, buf.as_mut_ptr(), &mut n) != 0;
            CloseHandle(p);
            ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
        }
    }

    fn windows() -> Vec<HWND> {
        unsafe extern "system" fn each(w: HWND, out: LPARAM) -> BOOL {
            unsafe { (*(out as *mut Vec<HWND>)).push(w) };
            1
        }
        let mut out: Vec<HWND> = vec![];
        unsafe { EnumWindows(Some(each), &mut out as *mut _ as LPARAM) };
        out
    }

    fn same_path(a: &str, b: &str) -> bool {
        let norm = |s: &str| s.replace('/', "\\").trim_start_matches("\\\\?\\").to_lowercase();
        norm(a) == norm(b)
    }

    /// An emulator installed by NX2 Launcher (its data is the shared one) whose main window names a game:
    /// these emulators put the game's file name in their title, and NX2 Launcher's names hold the program id.
    fn target(exes: &[String]) -> Option<Target> {
        windows().into_iter().find_map(|w| unsafe {
            if IsWindowVisible(w) == 0 || !GetWindow(w, GW_OWNER).is_null() { return None; }
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(w, buf.as_mut_ptr(), buf.len() as i32);
            let id = crate::config::game_id(&String::from_utf16_lossy(&buf[..n.max(0) as usize]))?;
            let pid = pid_of(w);
            let exe = exe_of(pid)?;
            exes.iter().any(|e| same_path(e, &exe)).then_some(Target { pid, main: w, id })
        })
    }

    fn emulator_exes() -> Vec<String> {
        crate::emu::emulator_folders().iter().filter_map(|f| crate::emu::find_exe(f)).map(|p| p.to_string_lossy().into_owned()).collect()
    }

    pub fn request(game: Option<&str>) -> bool {
        if DLL.is_empty() || !crate::config::live_shortcut_ready() { return false; }
        let Some(t) = target(&emulator_exes()) else { return false };
        if game.is_some_and(|g| crate::config::game_id(g).as_deref() != Some(t.id.as_str())) { return false; }
        AGAIN.store(true, Ordering::SeqCst);
        crate::emit_status("Réglage enregistré : il sera appliqué au jeu dès que tu y reviens.");
        if !BUSY.swap(true, Ordering::SeqCst) {
            std::thread::spawn(|| loop {
                while AGAIN.swap(false, Ordering::SeqCst) { once(); }
                BUSY.store(false, Ordering::SeqCst);
                // A request made just after the last check found nobody at work.
                if !AGAIN.load(Ordering::SeqCst) || BUSY.swap(true, Ordering::SeqCst) { break; }
            });
        }
        true
    }

    pub fn wait(limit: Duration) {
        let start = Instant::now();
        while BUSY.load(Ordering::SeqCst) && start.elapsed() < limit { std::thread::sleep(Duration::from_millis(100)); }
    }

    /// Waits for the player to be back in the game, then applies everything saved so far.
    fn once() {
        let exes = emulator_exes();
        let t = loop {
            let Some(t) = target(&exes) else { return };
            let front = || pid_of(unsafe { GetForegroundWindow() }) == t.pid;
            if front() {
                // Settled there, not passing through while switching windows.
                std::thread::sleep(Duration::from_millis(500));
                if front() { break t; }
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        // What is saved from now on is applied here too.
        AGAIN.store(false, Ordering::SeqCst);
        match apply(&t) {
            Ok(()) => {
                crate::log::info("settings applied during the game");
                crate::emit_status("Réglage appliqué au jeu en cours.");
            }
            Err(e) => {
                crate::log::error(&format!("settings during the game: {e}"));
                crate::emit_status("Le réglage n'a pas pu être appliqué pendant la partie : il le sera au prochain lancement du jeu.");
            }
        }
    }

    fn apply(t: &Target) -> Result<()> {
        let path = crate::config::game_ini_of(&t.id);
        let restore = crate::config::game_file_for_live(&t.id)?;
        let written = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let result = open_and_accept(t);
        if let Some(before) = restore {
            // The emulator writes the file once its window is gone: the common values stay out of it.
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(3) && std::fs::metadata(&path).and_then(|m| m.modified()).ok() == written {
                std::thread::sleep(Duration::from_millis(50));
            }
            std::thread::sleep(Duration::from_millis(200));
            crate::config::put_back_game_file(&t.id, before)?;
        }
        result
    }

    /// The DLL, written once per version next to NX2 Launcher's settings. An emulator keeps it loaded until it closes.
    fn dll_path() -> Result<PathBuf> {
        use sha2::{Digest, Sha256};
        let hash: String = Sha256::digest(DLL).iter().take(4).map(|b| format!("{b:02x}")).collect();
        let dir = manager_dir();
        let path = dir.join(format!("nx2-quiet-{hash}.dll"));
        if std::fs::read(&path).ok().as_deref() != Some(DLL) {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(&path, DLL)?;
        }
        // Older versions, unless an emulator still holds one.
        for e in std::fs::read_dir(&dir)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("nx2-quiet-") && name.ends_with(".dll") && e.path() != path { let _ = std::fs::remove_file(e.path()); }
        }
        Ok(path)
    }

    /// Everything that must be undone, whatever happens.
    struct Hooks {
        event_hook: HWINEVENTHOOK,
        message_hook: HHOOK,
        ready: HANDLE,
        armed: HANDLE,
    }

    impl Drop for Hooks {
        fn drop(&mut self) {
            unsafe {
                if !self.armed.is_null() { ResetEvent(self.armed); CloseHandle(self.armed); }
                if !self.event_hook.is_null() { UnhookWinEvent(self.event_hook); }
                if !self.message_hook.is_null() { UnhookWindowsHookEx(self.message_hook); }
                if !self.ready.is_null() { CloseHandle(self.ready); }
            }
        }
    }

    fn open_and_accept(t: &Target) -> Result<()> {
        let dll = dll_path()?;
        let mut hooks = Hooks { event_hook: std::ptr::null_mut(), message_hook: std::ptr::null_mut(), ready: std::ptr::null_mut(), armed: std::ptr::null_mut() };
        unsafe {
            let module = LoadLibraryW(wide(&dll.to_string_lossy()).as_ptr());
            if module.is_null() { bail!("nx2-quiet not loaded"); }
            let (Some(event), Some(load)) = (GetProcAddress(module, c"nx2_event".as_ptr().cast()), GetProcAddress(module, c"nx2_load".as_ptr().cast())) else {
                bail!("nx2-quiet incomplete");
            };
            hooks.ready = CreateEventW(std::ptr::null(), 1, 0, wide(&format!("Local\\nx2-quiet-ready-{}", t.pid)).as_ptr());
            // Windows loads the DLL into the emulator for these hooks; the message posted makes it happen now.
            type Event = unsafe extern "system" fn(HWINEVENTHOOK, u32, HWND, i32, i32, u32, u32);
            type Load = unsafe extern "system" fn(i32, usize, isize) -> isize;
            type Found = unsafe extern "system" fn() -> isize;
            let event = std::mem::transmute::<Found, Event>(event);
            let load = std::mem::transmute::<Found, Load>(load);
            hooks.event_hook = SetWinEventHook(EVENT_OBJECT_CREATE, EVENT_OBJECT_CREATE, module, Some(event), t.pid, 0, WINEVENT_INCONTEXT);
            let thread = GetWindowThreadProcessId(t.main, std::ptr::null_mut());
            hooks.message_hook = SetWindowsHookExW(WH_GETMESSAGE, Some(load), module, thread);
            if hooks.event_hook.is_null() || hooks.message_hook.is_null() { bail!("hooks refused"); }
            PostThreadMessageW(thread, WM_NULL, 0, 0);
            if WaitForSingleObject(hooks.ready, 3000) != WAIT_OBJECT_0 { bail!("nx2-quiet not ready in the emulator"); }
            hooks.armed = CreateEventW(std::ptr::null(), 1, 1, wide(&format!("Local\\nx2-quiet-{}", t.pid)).as_ptr());
            if hooks.armed.is_null() { bail!("could not arm nx2-quiet"); }
            // The player may have left the game meanwhile: the shortcut only works on the window in front.
            let front = GetForegroundWindow();
            if pid_of(front) != t.pid { bail!("the game is no longer in front"); }
            let scan = (MapVirtualKeyW(VK_F13 as u32, 0) as isize) << 16;
            PostMessageW(front, WM_KEYDOWN, VK_F13 as usize, 1 | scan);
            PostMessageW(front, WM_KEYUP, VK_F13 as usize, 1 | scan | (3 << 30));
        }
        let dialog = wait_for(Duration::from_secs(10), || owned(t)).ok_or_else(|| anyhow!("the game settings window did not open"))?;
        let pressed = press_ok(dialog);
        let closed = wait_for(Duration::from_secs(5), || (unsafe { IsWindow(dialog) } == 0 || owned(t) != Some(dialog)).then_some(())).is_some();
        if !closed {
            // Never left open and invisible: closed as its own close button would, nothing applied.
            unsafe { PostMessageW(dialog, WM_CLOSE, 0, 0) };
            bail!("the game settings window stayed open ({})", pressed.err().map_or("OK pressed".to_string(), |e| e.to_string()));
        }
        drop(hooks);
        pressed
    }

    fn wait_for<T>(limit: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
        let start = Instant::now();
        loop {
            if let Some(v) = f() { return Some(v); }
            if start.elapsed() > limit { return None; }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The window the emulator opened over its main window.
    fn owned(t: &Target) -> Option<HWND> {
        windows().into_iter().find(|&w| unsafe { pid_of(w) == t.pid && GetWindow(w, GW_OWNER) == t.main && IsWindowVisible(w) != 0 })
    }

    /// Presses the window's OK button. Qt names it "OK" in every language these emulators have; otherwise the first
    /// button of its button box, where Windows puts the one that accepts.
    fn press_ok(dialog: HWND) -> Result<()> {
        use windows::core::Interface;
        use windows::Win32::System::Variant::VARIANT;
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern, TreeScope_Descendants, UIA_ButtonControlTypeId,
            UIA_ControlTypePropertyId, UIA_InvokePatternId,
        };
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            // The window may still be building its pages: asked again until its buttons are there.
            let start = Instant::now();
            let buttons: Vec<IUIAutomationElement> = loop {
                let found = (|| -> windows::core::Result<Vec<IUIAutomationElement>> {
                    let el = uia.ElementFromHandle(windows::Win32::Foundation::HWND(dialog))?;
                    let cond = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_ButtonControlTypeId.0))?;
                    let all = el.FindAll(TreeScope_Descendants, &cond)?;
                    (0..all.Length()?).map(|i| all.GetElement(i)).collect()
                })();
                match found {
                    Ok(list) if !list.is_empty() => break list,
                    _ if start.elapsed() > Duration::from_secs(5) => bail!("no button found in the game settings window"),
                    _ => std::thread::sleep(Duration::from_millis(100)),
                }
            };
            let name = |b: &IUIAutomationElement| b.CurrentName().map(|n| n.to_string()).unwrap_or_default();
            let id = |b: &IUIAutomationElement| b.CurrentAutomationId().map(|n| n.to_string()).unwrap_or_default();
            let ok = buttons
                .iter()
                .find(|b| name(b).replace('&', "").eq_ignore_ascii_case("ok"))
                .or_else(|| buttons.iter().find(|b| id(b).to_lowercase().contains("buttonbox")))
                .ok_or_else(|| anyhow!("no OK button: {}", buttons.iter().map(|b| format!("{}|{}", name(b), id(b))).collect::<Vec<_>>().join(", ")))?;
            let invoke: IUIAutomationInvokePattern = ok.GetCurrentPattern(UIA_InvokePatternId)?.cast()?;
            invoke.Invoke()?;
        }
        Ok(())
    }
}
